use crate::kalshi_exec::KalshiExecutor;
use crate::polymarket_exec::PolymarketExecutor;
use chrono::Utc;
use pmhft_common::{ArbSignal, FillReport, LegInstruction, Platform};
use pmhft_risk::RiskManager;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// Dual-leg execution engine.
///
/// Receives ArbSignals, performs pre-trade risk checks, then submits
/// both legs concurrently (Polymarket via Dome, Kalshi via FIX or REST).
pub struct ExecutionEngine {
    risk_manager: Arc<RiskManager>,
    poly_executor: PolymarketExecutor,
    kalshi_executor: KalshiExecutor,
    signal_rx: mpsc::Receiver<ArbSignal>,
}

impl ExecutionEngine {
    pub fn new(
        risk_manager: Arc<RiskManager>,
        poly_executor: PolymarketExecutor,
        kalshi_executor: KalshiExecutor,
        signal_rx: mpsc::Receiver<ArbSignal>,
    ) -> Self {
        Self {
            risk_manager,
            poly_executor,
            kalshi_executor,
            signal_rx,
        }
    }

    /// Run the execution loop, processing signals as they arrive.
    pub async fn run(&mut self) {
        info!("Execution engine started");

        while let Some(signal) = self.signal_rx.recv().await {
            // 1. Check signal freshness.
            let age_ms = (Utc::now() - signal.timestamp).num_milliseconds() as u64;
            if age_ms > signal.ttl_ms {
                pmhft_telemetry::inc_counter("pmhft.execution.signals_expired");
                continue;
            }

            // 2. Pre-trade risk check.
            if let Err(e) = self.risk_manager.pre_trade_check(&signal) {
                warn!(
                    signal_id = %signal.signal_id,
                    error = %e,
                    "Pre-trade risk check failed"
                );
                pmhft_telemetry::inc_counter("pmhft.execution.signals_risk_rejected");
                continue;
            }

            // 3. Execute both legs concurrently.
            self.risk_manager.orders_submitted(2);

            let poly_leg = signal.poly_side.clone();
            let kalshi_leg = signal.kalshi_side.clone();

            let (poly_result, kalshi_result) = tokio::join!(
                self.poly_executor.execute(&poly_leg),
                self.kalshi_executor.execute(&kalshi_leg),
            );

            self.risk_manager.orders_closed(2);

            // 4. Handle results.
            match (poly_result, kalshi_result) {
                (Ok(poly_fill), Ok(kalshi_fill)) => {
                    self.risk_manager
                        .post_trade_update(&poly_fill, &kalshi_fill);
                    pmhft_telemetry::inc_counter("pmhft.execution.successful_arbs");
                    info!(
                        signal_id = %signal.signal_id,
                        signal_type = ?signal.signal_type,
                        edge_bps = %signal.edge_bps,
                        expected_pnl = %signal.expected_pnl,
                        "Arbitrage executed successfully"
                    );
                }
                (Ok(poly_fill), Err(kalshi_err)) => {
                    error!(
                        signal_id = %signal.signal_id,
                        error = %kalshi_err,
                        "Kalshi leg failed, Polymarket leg filled — initiating unwind"
                    );
                    pmhft_telemetry::inc_counter("pmhft.execution.partial_fills");
                    self.unwind_leg(&poly_fill).await;
                }
                (Err(poly_err), Ok(kalshi_fill)) => {
                    error!(
                        signal_id = %signal.signal_id,
                        error = %poly_err,
                        "Polymarket leg failed, Kalshi leg filled — initiating unwind"
                    );
                    pmhft_telemetry::inc_counter("pmhft.execution.partial_fills");
                    self.unwind_kalshi_leg(&kalshi_fill).await;
                }
                (Err(poly_err), Err(kalshi_err)) => {
                    warn!(
                        signal_id = %signal.signal_id,
                        poly_err = %poly_err,
                        kalshi_err = %kalshi_err,
                        "Both legs failed — no action needed"
                    );
                    pmhft_telemetry::inc_counter("pmhft.execution.both_legs_failed");
                }
            }
        }

        info!("Execution engine stopped");
    }

    /// Unwind a Polymarket fill by reversing the position.
    async fn unwind_leg(&self, fill: &FillReport) {
        let unwind = LegInstruction {
            platform: fill.market_id.platform,
            market_id: fill.market_id.clone(),
            direction: fill.direction.opposite(),
            side: fill.side,
            limit_price: fill.fill_price, // Market unwind at fill price.
            quantity: fill.filled_quantity,
        };

        match fill.market_id.platform {
            Platform::Polymarket => {
                if let Err(e) = self.poly_executor.execute(&unwind).await {
                    error!(error = %e, "Failed to unwind Polymarket leg");
                }
            }
            Platform::Kalshi => {
                if let Err(e) = self.kalshi_executor.execute(&unwind).await {
                    error!(error = %e, "Failed to unwind Kalshi leg");
                }
            }
        }
    }

    /// Unwind a Kalshi fill.
    async fn unwind_kalshi_leg(&self, fill: &FillReport) {
        let unwind = LegInstruction {
            platform: Platform::Kalshi,
            market_id: fill.market_id.clone(),
            direction: fill.direction.opposite(),
            side: fill.side,
            limit_price: fill.fill_price,
            quantity: fill.filled_quantity,
        };

        if let Err(e) = self.kalshi_executor.execute(&unwind).await {
            error!(error = %e, "Failed to unwind Kalshi leg");
        }
    }
}
