use dashmap::DashMap;
use pmhft_common::{Direction, FillReport, Platform};
use rust_decimal::Decimal;
use std::sync::atomic::{AtomicI64, Ordering};

/// Tracks positions, PnL, and exposure across all markets and platforms.
pub struct PositionTracker {
    /// (Platform, market_id) -> signed position (positive = long, negative = short).
    positions: DashMap<(Platform, String), Decimal>,
    /// Realized PnL in microdollars (for atomic operations).
    realized_pnl_micro: AtomicI64,
    /// Daily loss accumulator in microdollars (resets at UTC midnight).
    daily_loss_micro: AtomicI64,
}

impl PositionTracker {
    pub fn new() -> Self {
        Self {
            positions: DashMap::new(),
            realized_pnl_micro: AtomicI64::new(0),
            daily_loss_micro: AtomicI64::new(0),
        }
    }

    /// Update position from a fill report.
    pub fn update_from_fill(&self, fill: &FillReport) {
        let key = (fill.market_id.platform, fill.market_id.id.clone());
        let delta = match fill.direction {
            Direction::Buy => fill.filled_quantity,
            Direction::Sell => -fill.filled_quantity,
        };

        self.positions
            .entry(key)
            .and_modify(|pos| *pos += delta)
            .or_insert(delta);
    }

    /// Get net position for a specific market on a platform.
    pub fn net_position(&self, platform: Platform, market_id: &str) -> Decimal {
        self.positions
            .get(&(platform, market_id.to_string()))
            .map(|p| *p.value())
            .unwrap_or(Decimal::ZERO)
    }

    /// Total gross exposure across all positions (sum of absolute values).
    pub fn gross_exposure(&self) -> Decimal {
        self.positions
            .iter()
            .map(|entry| entry.value().abs())
            .sum()
    }

    /// Record realized PnL.
    pub fn record_pnl(&self, pnl_dollars: Decimal) {
        let micro = (pnl_dollars * Decimal::from(1_000_000))
            .to_string()
            .parse::<i64>()
            .unwrap_or(0);
        self.realized_pnl_micro
            .fetch_add(micro, Ordering::Relaxed);
        if micro < 0 {
            self.daily_loss_micro
                .fetch_add(micro, Ordering::Relaxed);
        }
    }

    /// Get realized PnL in dollars.
    pub fn realized_pnl(&self) -> Decimal {
        Decimal::from(self.realized_pnl_micro.load(Ordering::Relaxed))
            / Decimal::from(1_000_000)
    }

    /// Get daily loss in dollars (negative number).
    pub fn daily_loss(&self) -> Decimal {
        Decimal::from(self.daily_loss_micro.load(Ordering::Relaxed))
            / Decimal::from(1_000_000)
    }

    /// Reset daily loss accumulator (call at UTC midnight).
    pub fn reset_daily_loss(&self) {
        self.daily_loss_micro.store(0, Ordering::Relaxed);
    }

    /// Number of markets with open positions.
    pub fn open_position_count(&self) -> usize {
        self.positions
            .iter()
            .filter(|entry| *entry.value() != Decimal::ZERO)
            .count()
    }
}

impl Default for PositionTracker {
    fn default() -> Self {
        Self::new()
    }
}
