use dashmap::DashMap;
use pmhft_common::{MarketId, NormalizedQuote};
use pmhft_orderbook::L2Book;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::warn;

/// Unified market data aggregator.
///
/// Merges quotes from all feeds (Polymarket WS, Kalshi REST polling)
/// into a single normalized stream and maintains an in-memory orderbook cache.
pub struct MarketDataAggregator {
    /// Incoming quote channel from all feeds.
    quote_rx: broadcast::Receiver<NormalizedQuote>,
    /// Outgoing unified quote channel to downstream consumers (strategy engine).
    unified_tx: broadcast::Sender<NormalizedQuote>,
    /// In-memory orderbook cache: MarketId -> L2Book.
    book_cache: Arc<DashMap<String, L2Book>>,
    /// Latest quote per market for quick access.
    quote_cache: Arc<DashMap<String, NormalizedQuote>>,
}

impl MarketDataAggregator {
    pub fn new(
        quote_rx: broadcast::Receiver<NormalizedQuote>,
    ) -> (Self, broadcast::Receiver<NormalizedQuote>) {
        let (unified_tx, unified_rx) = broadcast::channel(8192);
        let agg = Self {
            quote_rx,
            unified_tx,
            book_cache: Arc::new(DashMap::new()),
            quote_cache: Arc::new(DashMap::new()),
        };
        (agg, unified_rx)
    }

    pub fn book_cache(&self) -> Arc<DashMap<String, L2Book>> {
        self.book_cache.clone()
    }

    pub fn quote_cache(&self) -> Arc<DashMap<String, NormalizedQuote>> {
        self.quote_cache.clone()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<NormalizedQuote> {
        self.unified_tx.subscribe()
    }

    /// Get the latest quote for a market.
    pub fn latest_quote(&self, market_id: &MarketId) -> Option<NormalizedQuote> {
        self.quote_cache
            .get(&market_id.to_string())
            .map(|q| q.clone())
    }

    /// Run the aggregator loop.
    pub async fn run(&mut self) {
        loop {
            match self.quote_rx.recv().await {
                Ok(quote) => {
                    let key = quote.market_id.to_string();

                    // Update quote cache.
                    self.quote_cache.insert(key.clone(), quote.clone());

                    // Update orderbook cache if we have price level data.
                    if quote.best_bid.is_some() || quote.best_ask.is_some() {
                        let mut book = self
                            .book_cache
                            .entry(key)
                            .or_insert_with(L2Book::new);

                        // Apply the quote as a point update to the book.
                        let mut bid_updates = Vec::new();
                        let mut ask_updates = Vec::new();

                        if let Some(bid) = &quote.best_bid {
                            bid_updates.push((bid.price, bid.size));
                        }
                        if let Some(ask) = &quote.best_ask {
                            ask_updates.push((ask.price, ask.size));
                        }

                        book.apply_delta(
                            &bid_updates,
                            &ask_updates,
                            quote.timestamp,
                            quote.sequence,
                        );
                    }

                    // Forward to downstream consumers.
                    let _ = self.unified_tx.send(quote);
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(skipped = n, "Market data aggregator lagged");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    warn!("Market data aggregator input channel closed");
                    break;
                }
            }
        }
    }
}
