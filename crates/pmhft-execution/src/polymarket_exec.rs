use chrono::Utc;
use pmhft_common::config::ExecutionConfig;
use pmhft_common::{Direction, FillReport, LegInstruction, PmhftError, Result};
use pmhft_dome_client::rest::DomePolymarketClient;
use pmhft_dome_client::types::{DomePlaceOrderParams, DomePlaceOrderRequest};
use rust_decimal::Decimal;
use std::sync::Arc;
use tracing::info;

/// Executes Polymarket orders via Dome's placeOrder JSON-RPC endpoint.
pub struct PolymarketExecutor {
    dome_client: Arc<DomePolymarketClient>,
    config: ExecutionConfig,
}

impl PolymarketExecutor {
    pub fn new(dome_client: Arc<DomePolymarketClient>, config: ExecutionConfig) -> Self {
        Self {
            dome_client,
            config,
        }
    }

    /// Execute a leg instruction on Polymarket.
    pub async fn execute(&self, leg: &LegInstruction) -> Result<FillReport> {
        if !self.config.live_trading {
            return self.simulate_fill(leg);
        }

        let request = DomePlaceOrderRequest {
            jsonrpc: "2.0".into(),
            method: "placeOrder".into(),
            params: DomePlaceOrderParams {
                token_id: leg.market_id.id.clone(),
                side: match leg.direction {
                    Direction::Buy => "BUY".into(),
                    Direction::Sell => "SELL".into(),
                },
                price: leg.limit_price.to_string(),
                size: leg.quantity.to_string(),
                order_type: self.config.default_order_type.clone(),
                // Credentials are passed through Dome -- it needs the wallet
                // private key and L2 API credentials for signing.
                private_key: None,
                api_key: None,
                api_secret: None,
                api_passphrase: None,
            },
            id: 1,
        };

        let response = self.dome_client.place_order(&request).await?;

        // Check for JSON-RPC error.
        if let Some(error) = &response.error {
            return Err(PmhftError::OrderRejected {
                reason: error.message.clone().unwrap_or("Unknown error".into()),
            });
        }

        let result = response
            .result
            .ok_or_else(|| PmhftError::OrderRejected {
                reason: "No result in placeOrder response".into(),
            })?;

        info!(
            order_id = result.order_id.as_deref().unwrap_or("?"),
            status = result.status.as_deref().unwrap_or("?"),
            "Polymarket order placed via Dome"
        );

        Ok(FillReport {
            market_id: leg.market_id.clone(),
            direction: leg.direction,
            side: leg.side,
            filled_quantity: leg.quantity,
            fill_price: leg.limit_price,
            fees: leg.limit_price * leg.quantity * Decimal::new(2, 2), // ~2% taker fee
            external_order_id: result.order_id.unwrap_or_default(),
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
            "[PAPER] Polymarket simulated fill"
        );

        Ok(FillReport {
            market_id: leg.market_id.clone(),
            direction: leg.direction,
            side: leg.side,
            filled_quantity: leg.quantity,
            fill_price: leg.limit_price,
            fees: leg.limit_price * leg.quantity * Decimal::new(2, 2),
            external_order_id: format!("paper-{}", uuid::Uuid::new_v4()),
            timestamp: Utc::now(),
        })
    }
}
