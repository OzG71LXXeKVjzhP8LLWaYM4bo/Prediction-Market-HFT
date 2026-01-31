use crate::circuit_breaker::CircuitBreaker;
use crate::position_tracker::PositionTracker;
use pmhft_common::config::RiskConfig;
use pmhft_common::{ArbSignal, Direction, FillReport, PmhftError, Result};
use rust_decimal::Decimal;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// Pre-trade and post-trade risk management.
pub struct RiskManager {
    config: RiskConfig,
    positions: Arc<PositionTracker>,
    circuit_breaker: Arc<CircuitBreaker>,
    open_order_count: AtomicU32,
}

impl RiskManager {
    pub fn new(
        config: RiskConfig,
        positions: Arc<PositionTracker>,
        circuit_breaker: Arc<CircuitBreaker>,
    ) -> Self {
        Self {
            config,
            positions,
            circuit_breaker,
            open_order_count: AtomicU32::new(0),
        }
    }

    /// Pre-trade risk check. Must pass before any order is submitted.
    pub fn pre_trade_check(&self, signal: &ArbSignal) -> Result<()> {
        // 0. Circuit breaker.
        if self.circuit_breaker.is_active() {
            return Err(PmhftError::RiskLimitBreached(
                "Circuit breaker active".into(),
            ));
        }

        // 1. Position limit per market.
        for leg in [&signal.poly_side, &signal.kalshi_side] {
            let current =
                self.positions
                    .net_position(leg.platform, &leg.market_id.id);
            let new_pos = match leg.direction {
                Direction::Buy => current + leg.quantity,
                Direction::Sell => current - leg.quantity,
            };
            if new_pos.abs() > self.config.max_position_per_market {
                return Err(PmhftError::RiskLimitBreached(format!(
                    "Position limit for {}: |{}| > {}",
                    leg.market_id, new_pos, self.config.max_position_per_market
                )));
            }
        }

        // 2. Gross exposure limit.
        let additional =
            signal.poly_side.quantity * signal.poly_side.limit_price
            + signal.kalshi_side.quantity * signal.kalshi_side.limit_price;
        let current_exposure = self.positions.gross_exposure();
        if current_exposure + additional > self.config.max_gross_exposure_usd {
            return Err(PmhftError::RiskLimitBreached(format!(
                "Gross exposure: {} + {} > {}",
                current_exposure, additional, self.config.max_gross_exposure_usd
            )));
        }

        // 3. Per-order notional limit.
        let poly_notional = signal.poly_side.limit_price * signal.poly_side.quantity;
        let kalshi_notional = signal.kalshi_side.limit_price * signal.kalshi_side.quantity;
        if poly_notional > self.config.max_order_notional_usd {
            return Err(PmhftError::RiskLimitBreached(format!(
                "Poly order notional {} > {}",
                poly_notional, self.config.max_order_notional_usd
            )));
        }
        if kalshi_notional > self.config.max_order_notional_usd {
            return Err(PmhftError::RiskLimitBreached(format!(
                "Kalshi order notional {} > {}",
                kalshi_notional, self.config.max_order_notional_usd
            )));
        }

        // 4. Open order count limit.
        if self.open_order_count.load(Ordering::Relaxed) + 2 > self.config.max_open_orders {
            return Err(PmhftError::RiskLimitBreached(
                "Too many open orders".into(),
            ));
        }

        // 5. Daily loss limit.
        let daily_loss = self.positions.daily_loss();
        if daily_loss < self.config.max_daily_loss_usd {
            self.circuit_breaker
                .trip(&format!("Daily loss {} exceeded limit {}", daily_loss, self.config.max_daily_loss_usd));
            return Err(PmhftError::RiskLimitBreached(
                "Daily loss limit exceeded".into(),
            ));
        }

        Ok(())
    }

    /// Post-trade update after both legs are filled.
    pub fn post_trade_update(&self, poly_fill: &FillReport, kalshi_fill: &FillReport) {
        self.positions.update_from_fill(poly_fill);
        self.positions.update_from_fill(kalshi_fill);

        // Simple PnL estimate: sell price - buy price - fees.
        let pnl = (poly_fill.fill_price * poly_fill.filled_quantity)
            .checked_sub(poly_fill.fees)
            .unwrap_or(Decimal::ZERO)
            + (kalshi_fill.fill_price * kalshi_fill.filled_quantity)
                .checked_sub(kalshi_fill.fees)
                .unwrap_or(Decimal::ZERO);

        self.positions.record_pnl(pnl);
    }

    /// Increment open order count (call when orders are submitted).
    pub fn orders_submitted(&self, count: u32) {
        self.open_order_count.fetch_add(count, Ordering::Relaxed);
    }

    /// Decrement open order count (call when orders are filled/cancelled).
    pub fn orders_closed(&self, count: u32) {
        self.open_order_count.fetch_sub(count, Ordering::Relaxed);
    }

    pub fn positions(&self) -> &Arc<PositionTracker> {
        &self.positions
    }

    pub fn circuit_breaker(&self) -> &Arc<CircuitBreaker> {
        &self.circuit_breaker
    }
}
