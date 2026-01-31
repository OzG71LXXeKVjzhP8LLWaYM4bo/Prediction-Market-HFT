use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::collections::BTreeMap;

/// Price key that implements Ord for BTreeMap.
/// Wraps Decimal since Decimal's Ord is suitable for price ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PriceKey(pub Decimal);

impl PartialOrd for PriceKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PriceKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

/// A single level in the orderbook.
#[derive(Debug, Clone, Copy)]
pub struct Level {
    pub price: Decimal,
    pub size: Decimal,
}

/// L2 orderbook for a single market outcome (e.g., "Yes" on Polymarket token X).
///
/// Bids are stored ascending by price (best bid = last entry).
/// Asks are stored ascending by price (best ask = first entry).
pub struct L2Book {
    bids: BTreeMap<PriceKey, Decimal>,
    asks: BTreeMap<PriceKey, Decimal>,
    pub last_update: DateTime<Utc>,
    pub sequence: u64,
}

impl L2Book {
    pub fn new() -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            last_update: Utc::now(),
            sequence: 0,
        }
    }

    /// Apply a full snapshot (replaces entire book).
    pub fn apply_snapshot(
        &mut self,
        bids: &[(Decimal, Decimal)],
        asks: &[(Decimal, Decimal)],
        timestamp: DateTime<Utc>,
        seq: u64,
    ) {
        self.bids.clear();
        self.asks.clear();
        for &(price, size) in bids {
            if size > Decimal::ZERO {
                self.bids.insert(PriceKey(price), size);
            }
        }
        for &(price, size) in asks {
            if size > Decimal::ZERO {
                self.asks.insert(PriceKey(price), size);
            }
        }
        self.last_update = timestamp;
        self.sequence = seq;
    }

    /// Apply an incremental delta (update or remove levels).
    /// Stale deltas (sequence <= current) are silently dropped.
    pub fn apply_delta(
        &mut self,
        bid_updates: &[(Decimal, Decimal)],
        ask_updates: &[(Decimal, Decimal)],
        timestamp: DateTime<Utc>,
        seq: u64,
    ) {
        if seq <= self.sequence {
            return;
        }
        for &(price, size) in bid_updates {
            if size == Decimal::ZERO {
                self.bids.remove(&PriceKey(price));
            } else {
                self.bids.insert(PriceKey(price), size);
            }
        }
        for &(price, size) in ask_updates {
            if size == Decimal::ZERO {
                self.asks.remove(&PriceKey(price));
            } else {
                self.asks.insert(PriceKey(price), size);
            }
        }
        self.last_update = timestamp;
        self.sequence = seq;
    }

    /// Best bid (highest bid price).
    pub fn best_bid(&self) -> Option<Level> {
        self.bids
            .iter()
            .next_back()
            .map(|(k, &size)| Level { price: k.0, size })
    }

    /// Best ask (lowest ask price).
    pub fn best_ask(&self) -> Option<Level> {
        self.asks
            .iter()
            .next()
            .map(|(k, &size)| Level { price: k.0, size })
    }

    /// Mid price between best bid and best ask.
    pub fn mid_price(&self) -> Option<Decimal> {
        match (self.best_bid(), self.best_ask()) {
            (Some(bid), Some(ask)) => Some((bid.price + ask.price) / Decimal::TWO),
            _ => None,
        }
    }

    /// Bid-ask spread in absolute terms.
    pub fn spread(&self) -> Option<Decimal> {
        match (self.best_bid(), self.best_ask()) {
            (Some(bid), Some(ask)) => Some(ask.price - bid.price),
            _ => None,
        }
    }

    /// Compute total liquidity available on the ask side up to max_price.
    /// Returns (total_size, total_cost).
    pub fn ask_liquidity_up_to(&self, max_price: Decimal) -> (Decimal, Decimal) {
        let mut total_size = Decimal::ZERO;
        let mut total_cost = Decimal::ZERO;
        for (&PriceKey(price), &size) in &self.asks {
            if price > max_price {
                break;
            }
            total_size += size;
            total_cost += price * size;
        }
        (total_size, total_cost)
    }

    /// Compute total liquidity available on the bid side down to min_price.
    /// Returns (total_size, total_cost).
    pub fn bid_liquidity_down_to(&self, min_price: Decimal) -> (Decimal, Decimal) {
        let mut total_size = Decimal::ZERO;
        let mut total_cost = Decimal::ZERO;
        for (&PriceKey(price), &size) in self.bids.iter().rev() {
            if price < min_price {
                break;
            }
            total_size += size;
            total_cost += price * size;
        }
        (total_size, total_cost)
    }

    /// Age of the book in milliseconds since last update.
    pub fn age_ms(&self) -> i64 {
        (Utc::now() - self.last_update).num_milliseconds()
    }

    /// Number of bid levels.
    pub fn bid_depth(&self) -> usize {
        self.bids.len()
    }

    /// Number of ask levels.
    pub fn ask_depth(&self) -> usize {
        self.asks.len()
    }

    /// Whether the book has any data.
    pub fn is_empty(&self) -> bool {
        self.bids.is_empty() && self.asks.is_empty()
    }
}

impl Default for L2Book {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_empty_book() {
        let book = L2Book::new();
        assert!(book.best_bid().is_none());
        assert!(book.best_ask().is_none());
        assert!(book.mid_price().is_none());
        assert!(book.spread().is_none());
        assert!(book.is_empty());
    }

    #[test]
    fn test_apply_snapshot() {
        let mut book = L2Book::new();
        let bids = vec![
            (dec!(0.45), dec!(100)),
            (dec!(0.44), dec!(200)),
            (dec!(0.43), dec!(300)),
        ];
        let asks = vec![
            (dec!(0.47), dec!(150)),
            (dec!(0.48), dec!(250)),
            (dec!(0.50), dec!(350)),
        ];
        book.apply_snapshot(&bids, &asks, Utc::now(), 1);

        let bb = book.best_bid().unwrap();
        assert_eq!(bb.price, dec!(0.45));
        assert_eq!(bb.size, dec!(100));

        let ba = book.best_ask().unwrap();
        assert_eq!(ba.price, dec!(0.47));
        assert_eq!(ba.size, dec!(150));

        assert_eq!(book.mid_price().unwrap(), dec!(0.46));
        assert_eq!(book.spread().unwrap(), dec!(0.02));
        assert_eq!(book.bid_depth(), 3);
        assert_eq!(book.ask_depth(), 3);
    }

    #[test]
    fn test_apply_delta() {
        let mut book = L2Book::new();
        book.apply_snapshot(
            &[(dec!(0.45), dec!(100))],
            &[(dec!(0.47), dec!(150))],
            Utc::now(),
            1,
        );

        // Update: new bid level, remove ask level (size=0)
        book.apply_delta(
            &[(dec!(0.46), dec!(50))],
            &[(dec!(0.47), dec!(0))],
            Utc::now(),
            2,
        );

        assert_eq!(book.best_bid().unwrap().price, dec!(0.46));
        assert_eq!(book.best_bid().unwrap().size, dec!(50));
        assert!(book.best_ask().is_none());
    }

    #[test]
    fn test_stale_delta_ignored() {
        let mut book = L2Book::new();
        book.apply_snapshot(&[(dec!(0.50), dec!(100))], &[], Utc::now(), 5);

        // Stale delta with seq <= 5 should be ignored
        book.apply_delta(&[(dec!(0.50), dec!(0))], &[], Utc::now(), 3);

        assert_eq!(book.best_bid().unwrap().size, dec!(100));
    }

    #[test]
    fn test_zero_size_filtered_in_snapshot() {
        let mut book = L2Book::new();
        book.apply_snapshot(
            &[(dec!(0.50), dec!(0)), (dec!(0.49), dec!(100))],
            &[],
            Utc::now(),
            1,
        );
        assert_eq!(book.bid_depth(), 1);
        assert_eq!(book.best_bid().unwrap().price, dec!(0.49));
    }

    #[test]
    fn test_ask_liquidity() {
        let mut book = L2Book::new();
        book.apply_snapshot(
            &[],
            &[
                (dec!(0.50), dec!(100)),
                (dec!(0.51), dec!(200)),
                (dec!(0.55), dec!(300)),
            ],
            Utc::now(),
            1,
        );

        let (size, cost) = book.ask_liquidity_up_to(dec!(0.52));
        assert_eq!(size, dec!(300));
        assert_eq!(cost, dec!(0.50) * dec!(100) + dec!(0.51) * dec!(200));
    }

    #[test]
    fn test_bid_liquidity() {
        let mut book = L2Book::new();
        book.apply_snapshot(
            &[
                (dec!(0.45), dec!(100)),
                (dec!(0.44), dec!(200)),
                (dec!(0.40), dec!(300)),
            ],
            &[],
            Utc::now(),
            1,
        );

        let (size, cost) = book.bid_liquidity_down_to(dec!(0.43));
        assert_eq!(size, dec!(300));
        assert_eq!(cost, dec!(0.45) * dec!(100) + dec!(0.44) * dec!(200));
    }
}
