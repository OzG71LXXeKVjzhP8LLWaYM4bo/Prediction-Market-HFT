use pmhft_common::{MarketId, NormalizedQuote};

/// Events emitted by the market data aggregator.
#[derive(Debug, Clone)]
pub enum MarketDataEvent {
    /// A normalized quote update from any platform.
    QuoteUpdate(NormalizedQuote),
    /// An orderbook was fully reset (snapshot applied).
    BookReset {
        market_id: MarketId,
        sequence: u64,
    },
    /// A feed disconnected.
    FeedDisconnected {
        platform: pmhft_common::Platform,
        reason: String,
    },
    /// A feed reconnected.
    FeedReconnected {
        platform: pmhft_common::Platform,
    },
}
