use pmhft_common::Platform;
use rust_decimal::Decimal;

/// Fee model for cross-platform arbitrage calculations.
///
/// Accounts for taker/maker fees on both Polymarket and Kalshi
/// to compute net edge after all costs.
pub struct FeeModel {
    /// Polymarket taker fee rate (e.g., 0.02 = 2%).
    pub poly_taker_fee_rate: Decimal,
    /// Polymarket maker fee rate (e.g., 0.00 = 0%).
    pub poly_maker_fee_rate: Decimal,
    /// Kalshi taker fee per contract in cents.
    pub kalshi_taker_fee_cents: Decimal,
    /// Kalshi maker fee per contract in cents.
    pub kalshi_maker_fee_cents: Decimal,
}

impl FeeModel {
    /// Default fee structure.
    pub fn default_fees() -> Self {
        Self {
            poly_taker_fee_rate: Decimal::new(2, 2),  // 2%
            poly_maker_fee_rate: Decimal::ZERO,
            kalshi_taker_fee_cents: Decimal::new(7, 1), // ~$0.07 per contract
            kalshi_maker_fee_cents: Decimal::ZERO,
        }
    }

    /// Calculate net edge after fees for a cross-platform trade.
    ///
    /// `buy_price` and `sell_price` are in 0.00..1.00 probability space.
    /// Returns net edge in the same space (positive = profitable).
    pub fn net_edge(
        &self,
        buy_platform: Platform,
        buy_price: Decimal,
        sell_platform: Platform,
        sell_price: Decimal,
    ) -> Decimal {
        let gross_edge = sell_price - buy_price;
        let buy_fee = self.taker_fee(buy_platform, buy_price);
        let sell_fee = self.taker_fee(sell_platform, sell_price);
        gross_edge - buy_fee - sell_fee
    }

    /// Taker fee for a single leg.
    fn taker_fee(&self, platform: Platform, price: Decimal) -> Decimal {
        match platform {
            Platform::Polymarket => price * self.poly_taker_fee_rate,
            Platform::Kalshi => self.kalshi_taker_fee_cents / Decimal::from(100),
        }
    }

    /// Calculate the total cost of a complement arb (buy Yes on A + buy No on B).
    /// Includes fees on both legs.
    pub fn complement_cost(
        &self,
        yes_platform: Platform,
        yes_ask: Decimal,
        no_platform: Platform,
        no_ask: Decimal,
    ) -> Decimal {
        let yes_fee = self.taker_fee(yes_platform, yes_ask);
        let no_fee = self.taker_fee(no_platform, no_ask);
        yes_ask + no_ask + yes_fee + no_fee
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_net_edge_positive() {
        let fees = FeeModel::default_fees();
        let edge = fees.net_edge(
            Platform::Polymarket, dec!(0.50),
            Platform::Kalshi, dec!(0.56),
        );
        // gross = 0.06, poly fee = 0.50 * 0.02 = 0.01, kalshi fee = 0.007
        // net = 0.06 - 0.01 - 0.007 = 0.043
        assert!(edge > dec!(0.04));
        assert!(edge < dec!(0.05));
    }

    #[test]
    fn test_net_edge_negative() {
        let fees = FeeModel::default_fees();
        let edge = fees.net_edge(
            Platform::Polymarket, dec!(0.50),
            Platform::Kalshi, dec!(0.51),
        );
        // gross = 0.01, fees eat it up
        assert!(edge < Decimal::ZERO);
    }

    #[test]
    fn test_complement_cost() {
        let fees = FeeModel::default_fees();
        let cost = fees.complement_cost(
            Platform::Polymarket, dec!(0.48),
            Platform::Kalshi, dec!(0.48),
        );
        // cost = 0.48 + 0.48 + (0.48 * 0.02) + 0.007 = 0.96 + 0.0096 + 0.007 = 0.9766
        assert!(cost < Decimal::ONE);
    }
}
