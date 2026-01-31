use chrono::Utc;
use pmhft_common::config::ExecutionConfig;
use pmhft_common::{Direction, FillReport, LegInstruction, OutcomeSide, PmhftError, Result};
use pmhft_kalshi_client::fix::messages::{side as fix_side, time_in_force, ExecutionReport};
use pmhft_kalshi_client::fix::FixSession;
use pmhft_kalshi_client::rest::KalshiRestClient;
use pmhft_kalshi_client::types::KalshiCreateOrderRequest;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::time::{timeout, Duration};
use tracing::{info, warn};

/// Executes Kalshi orders via FIX 4.4 (primary) or REST (fallback).
pub struct KalshiExecutor {
    fix_session: Option<Arc<FixSession>>,
    rest_client: Option<Arc<KalshiRestClient>>,
    config: ExecutionConfig,
}

impl KalshiExecutor {
    pub fn new(
        fix_session: Option<Arc<FixSession>>,
        rest_client: Option<Arc<KalshiRestClient>>,
        config: ExecutionConfig,
    ) -> Self {
        Self {
            fix_session,
            rest_client,
            config,
        }
    }

    /// Execute a leg instruction on Kalshi.
    pub async fn execute(&self, leg: &LegInstruction) -> Result<FillReport> {
        if !self.config.live_trading {
            return self.simulate_fill(leg);
        }

        if self.config.use_fix_for_kalshi {
            if let Some(session) = &self.fix_session {
                return self.execute_via_fix(session, leg).await;
            }
        }

        if let Some(rest) = &self.rest_client {
            return self.execute_via_rest(rest, leg).await;
        }

        Err(PmhftError::KalshiFix(
            "No Kalshi execution path available (neither FIX nor REST configured)".into(),
        ))
    }

    /// Execute via FIX 4.4 — lowest latency path.
    async fn execute_via_fix(
        &self,
        session: &Arc<FixSession>,
        leg: &LegInstruction,
    ) -> Result<FillReport> {
        let cl_ord_id = uuid::Uuid::new_v4().to_string();

        let side = match (leg.direction, leg.side) {
            (Direction::Buy, OutcomeSide::Yes) => fix_side::BUY,
            (Direction::Sell, OutcomeSide::Yes) => fix_side::SELL,
            (Direction::Buy, OutcomeSide::No) => fix_side::BUY,
            (Direction::Sell, OutcomeSide::No) => fix_side::SELL,
        };

        let price_cents = (leg.limit_price * Decimal::from(100))
            .round_dp(0)
            .to_u32()
            .unwrap_or(50);

        let quantity = leg.quantity.to_u32().unwrap_or(1);

        let tif = match self.config.default_order_type.as_str() {
            "FOK" => time_in_force::FOK,
            "FAK" => time_in_force::IOC,
            _ => time_in_force::GTC,
        };

        // Subscribe to execution reports before sending.
        let mut exec_rx = session.subscribe_exec_reports();

        // Send the order.
        session
            .send_new_order(&cl_ord_id, &leg.market_id.id, side, quantity, price_cents, tif)
            .await?;

        // Wait for execution report with timeout.
        let fill_timeout = Duration::from_millis(self.config.fill_timeout_ms);
        match timeout(fill_timeout, self.wait_for_fill(&mut exec_rx, &cl_ord_id)).await {
            Ok(Ok(report)) => {
                info!(
                    cl_ord_id = %cl_ord_id,
                    fill_qty = report.cum_qty,
                    "Kalshi FIX order filled"
                );
                Ok(FillReport {
                    market_id: leg.market_id.clone(),
                    direction: leg.direction,
                    side: leg.side,
                    filled_quantity: Decimal::from(report.cum_qty),
                    fill_price: Decimal::from(report.avg_px.unwrap_or(0.0) as i64)
                        / Decimal::from(100),
                    fees: Decimal::ZERO, // Kalshi fees are deducted server-side.
                    external_order_id: report.order_id,
                    timestamp: Utc::now(),
                })
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err(PmhftError::KalshiFix(format!(
                "FIX fill timeout for {}",
                cl_ord_id
            ))),
        }
    }

    async fn wait_for_fill(
        &self,
        rx: &mut broadcast::Receiver<ExecutionReport>,
        cl_ord_id: &str,
    ) -> Result<ExecutionReport> {
        loop {
            match rx.recv().await {
                Ok(report) => {
                    if report.cl_ord_id == cl_ord_id {
                        match report.ord_status.as_str() {
                            "2" => return Ok(report), // Filled
                            "1" => continue,          // Partially filled, keep waiting
                            "8" => {
                                return Err(PmhftError::OrderRejected {
                                    reason: report.text.unwrap_or("Rejected".into()),
                                })
                            }
                            "4" => {
                                return Err(PmhftError::OrderRejected {
                                    reason: "Cancelled".into(),
                                })
                            }
                            _ => continue,
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(skipped = n, "FIX exec report channel lagged");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    return Err(PmhftError::KalshiFix(
                        "FIX exec report channel closed".into(),
                    ));
                }
            }
        }
    }

    /// Execute via REST API — fallback for when FIX is unavailable.
    async fn execute_via_rest(
        &self,
        client: &Arc<KalshiRestClient>,
        leg: &LegInstruction,
    ) -> Result<FillReport> {
        let price_cents = (leg.limit_price * Decimal::from(100))
            .round_dp(0)
            .to_u32()
            .unwrap_or(50);

        let tif = match self.config.default_order_type.as_str() {
            "FOK" => Some("fill_or_kill".to_string()),
            "FAK" => Some("immediate_or_cancel".to_string()),
            _ => None,
        };

        let request = KalshiCreateOrderRequest {
            ticker: leg.market_id.id.clone(),
            side: match leg.side {
                OutcomeSide::Yes => "yes".into(),
                OutcomeSide::No => "no".into(),
            },
            action: match leg.direction {
                Direction::Buy => "buy".into(),
                Direction::Sell => "sell".into(),
            },
            client_order_id: uuid::Uuid::new_v4().to_string(),
            count: leg.quantity.to_u32().unwrap_or(1),
            order_type: "limit".into(),
            yes_price: if leg.side == OutcomeSide::Yes {
                Some(price_cents)
            } else {
                None
            },
            no_price: if leg.side == OutcomeSide::No {
                Some(price_cents)
            } else {
                None
            },
            time_in_force: tif,
            post_only: Some(false),
        };

        let response = client.create_order(&request).await?;

        Ok(FillReport {
            market_id: leg.market_id.clone(),
            direction: leg.direction,
            side: leg.side,
            filled_quantity: Decimal::from(response.order.fill_count),
            fill_price: Decimal::from(response.order.yes_price.unwrap_or(0)) / Decimal::from(100),
            fees: Decimal::from(
                response.order.taker_fees.unwrap_or(0) + response.order.maker_fees.unwrap_or(0),
            ) / Decimal::from(100),
            external_order_id: response.order.order_id,
            timestamp: Utc::now(),
        })
    }

    /// Simulate a fill for paper trading.
    fn simulate_fill(&self, leg: &LegInstruction) -> Result<FillReport> {
        info!(
            market_id = %leg.market_id,
            direction = ?leg.direction,
            price = %leg.limit_price,
            qty = %leg.quantity,
            "[PAPER] Kalshi simulated fill"
        );

        Ok(FillReport {
            market_id: leg.market_id.clone(),
            direction: leg.direction,
            side: leg.side,
            filled_quantity: leg.quantity,
            fill_price: leg.limit_price,
            fees: Decimal::new(7, 3), // ~$0.007 per contract
            external_order_id: format!("paper-{}", uuid::Uuid::new_v4()),
            timestamp: Utc::now(),
        })
    }
}
