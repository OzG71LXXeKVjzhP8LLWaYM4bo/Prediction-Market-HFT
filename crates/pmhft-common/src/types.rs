use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Identifies which prediction market platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Platform {
    Polymarket,
    Kalshi,
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Platform::Polymarket => write!(f, "Polymarket"),
            Platform::Kalshi => write!(f, "Kalshi"),
        }
    }
}

/// The side of a binary outcome market.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OutcomeSide {
    Yes,
    No,
}

/// Trade direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Direction {
    Buy,
    Sell,
}

impl Direction {
    pub fn opposite(self) -> Self {
        match self {
            Direction::Buy => Direction::Sell,
            Direction::Sell => Direction::Buy,
        }
    }
}

/// A unique identifier for a market on any platform.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MarketId {
    pub platform: Platform,
    /// For Polymarket: token_id (the large numeric string).
    /// For Kalshi: market_ticker (e.g., "KXBTC-26JAN31-T104999.99").
    pub id: String,
}

impl std::fmt::Display for MarketId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.platform, self.id)
    }
}

/// A cross-platform matched pair -- the key abstraction for arbitrage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchedPair {
    pub pair_id: String,
    pub polymarket: PolymarketMarketRef,
    pub kalshi: KalshiMarketRef,
    pub match_confidence: f64,
    pub category: MarketCategory,
    pub discovered_at: DateTime<Utc>,
    pub last_verified: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolymarketMarketRef {
    pub market_slug: String,
    pub condition_id: String,
    /// token_ids[0] = Yes outcome, token_ids[1] = No outcome.
    pub token_ids: Vec<String>,
    pub question: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KalshiMarketRef {
    pub event_ticker: String,
    pub market_tickers: Vec<String>,
    pub title: String,
}

/// Market category for matching and filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MarketCategory {
    Sports,
    Politics,
    Crypto,
    Weather,
    Economics,
    Entertainment,
    Science,
    Other,
}

/// A normalized price quote from either platform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedQuote {
    pub market_id: MarketId,
    pub side: OutcomeSide,
    pub best_bid: Option<PriceLevel>,
    pub best_ask: Option<PriceLevel>,
    pub mid_price: Option<Decimal>,
    pub timestamp: DateTime<Utc>,
    pub sequence: u64,
}

/// A single price level on the orderbook.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PriceLevel {
    /// Price as a probability 0.00..1.00 (or equivalently, cents 0..100).
    pub price: Decimal,
    /// Available quantity at this level.
    pub size: Decimal,
}

/// An arbitrage signal emitted by the strategy engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbSignal {
    pub signal_id: uuid::Uuid,
    pub pair_id: String,
    pub signal_type: ArbSignalType,
    /// Edge in basis points after fees.
    pub edge_bps: Decimal,
    /// Confidence level 0.0 .. 1.0.
    pub confidence: Decimal,
    pub poly_side: LegInstruction,
    pub kalshi_side: LegInstruction,
    pub expected_pnl: Decimal,
    pub timestamp: DateTime<Utc>,
    /// Signal validity window in milliseconds.
    pub ttl_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArbSignalType {
    /// Direct price discrepancy (buy cheap on A, sell expensive on B).
    PriceArb,
    /// Yes + No across platforms sums to < 1.00 (locked profit).
    ComplementArb,
    /// Statistical mean-reversion of the cross-platform spread.
    StatisticalArb,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegInstruction {
    pub platform: Platform,
    pub market_id: MarketId,
    pub direction: Direction,
    pub side: OutcomeSide,
    pub limit_price: Decimal,
    pub quantity: Decimal,
}

/// Report of a filled order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FillReport {
    pub market_id: MarketId,
    pub direction: Direction,
    pub side: OutcomeSide,
    pub filled_quantity: Decimal,
    pub fill_price: Decimal,
    pub fees: Decimal,
    pub external_order_id: String,
    pub timestamp: DateTime<Utc>,
}

/// Represents an order submitted to a platform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub order_id: uuid::Uuid,
    pub external_id: Option<String>,
    pub client_order_id: String,
    pub market_id: MarketId,
    pub direction: Direction,
    pub side: OutcomeSide,
    pub price: Decimal,
    pub quantity: Decimal,
    pub order_type: OrderType,
    pub status: OrderStatus,
    pub filled_quantity: Decimal,
    pub avg_fill_price: Option<Decimal>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType {
    /// Good Till Cancelled.
    GTC,
    /// Fill or Kill.
    FOK,
    /// Fill and Kill (Immediate or Cancel).
    FAK,
}

impl OrderType {
    pub fn from_str_config(s: &str) -> Self {
        match s {
            "FOK" => OrderType::FOK,
            "FAK" => OrderType::FAK,
            _ => OrderType::GTC,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderStatus {
    Pending,
    Submitted,
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
    Expired,
}
