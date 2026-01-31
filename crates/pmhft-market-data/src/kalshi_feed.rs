use chrono::Utc;
use pmhft_common::{MarketId, NormalizedQuote, OutcomeSide, Platform, PriceLevel};
use pmhft_dome_client::rest::DomeKalshiClient;
use pmhft_dome_client::types::DomeKalshiOrderbook;
use pmhft_matching::PairRegistry;
use rust_decimal::prelude::FromPrimitive;
use rust_decimal::Decimal;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::time::{self, Duration};
use tracing::warn;

/// Polls Kalshi orderbook data via Dome REST and emits NormalizedQuote updates.
///
/// With the free tier (1 req/s), this round-robins through active Kalshi pairs.
/// At higher tiers, it can poll more frequently.
pub struct KalshiFeed {
    dome_kalshi: DomeKalshiClient,
    registry: Arc<PairRegistry>,
    quote_tx: broadcast::Sender<NormalizedQuote>,
    poll_interval: Duration,
}

impl KalshiFeed {
    pub fn new(
        dome_kalshi: DomeKalshiClient,
        registry: Arc<PairRegistry>,
        quote_tx: broadcast::Sender<NormalizedQuote>,
        poll_interval: Duration,
    ) -> Self {
        Self {
            dome_kalshi,
            registry,
            quote_tx,
            poll_interval,
        }
    }

    /// Run the polling loop, round-robining through active Kalshi tickers.
    pub async fn run(&self) {
        let mut interval = time::interval(self.poll_interval);

        loop {
            interval.tick().await;

            let tickers = self.registry.all_kalshi_tickers();
            if tickers.is_empty() {
                continue;
            }

            // Round-robin: poll one ticker per interval to stay within rate limits.
            for ticker in &tickers {
                interval.tick().await;

                match self.dome_kalshi.get_orderbook(ticker, None).await {
                    Ok(book) => {
                        self.emit_quotes_from_orderbook(&book);
                    }
                    Err(e) => {
                        warn!(ticker = ticker, error = %e, "Failed to poll Kalshi orderbook");
                    }
                }
            }
        }
    }

    fn emit_quotes_from_orderbook(&self, book: &DomeKalshiOrderbook) {
        let ticker = match book.ticker.as_ref() {
            Some(t) => t,
            None => return,
        };

        let timestamp = Utc::now();

        // Yes side
        let (best_bid, best_ask) = extract_bbo(&book.yes_bids, &book.yes_asks);
        let quote = NormalizedQuote {
            market_id: MarketId {
                platform: Platform::Kalshi,
                id: ticker.clone(),
            },
            side: OutcomeSide::Yes,
            best_bid,
            best_ask,
            mid_price: compute_mid(best_bid, best_ask),
            timestamp,
            sequence: timestamp.timestamp_millis() as u64,
        };
        let _ = self.quote_tx.send(quote);

        // No side
        let (best_bid, best_ask) = extract_bbo(&book.no_bids, &book.no_asks);
        let quote = NormalizedQuote {
            market_id: MarketId {
                platform: Platform::Kalshi,
                id: ticker.clone(),
            },
            side: OutcomeSide::No,
            best_bid,
            best_ask,
            mid_price: compute_mid(best_bid, best_ask),
            timestamp,
            sequence: timestamp.timestamp_millis() as u64,
        };
        let _ = self.quote_tx.send(quote);
    }
}

fn extract_bbo(
    bids: &[pmhft_dome_client::types::DomeOrderbookLevel],
    asks: &[pmhft_dome_client::types::DomeOrderbookLevel],
) -> (Option<PriceLevel>, Option<PriceLevel>) {
    // Find best bid (highest price).
    let best_bid = bids
        .iter()
        .filter_map(|l| {
            let price = Decimal::from_f64(l.price?)?;
            let size = Decimal::from_f64(l.size?)?;
            Some(PriceLevel { price, size })
        })
        .max_by(|a, b| a.price.cmp(&b.price));

    // Find best ask (lowest price).
    let best_ask = asks
        .iter()
        .filter_map(|l| {
            let price = Decimal::from_f64(l.price?)?;
            let size = Decimal::from_f64(l.size?)?;
            Some(PriceLevel { price, size })
        })
        .min_by(|a, b| a.price.cmp(&b.price));

    (best_bid, best_ask)
}

fn compute_mid(bid: Option<PriceLevel>, ask: Option<PriceLevel>) -> Option<Decimal> {
    match (bid, ask) {
        (Some(b), Some(a)) => Some((b.price + a.price) / Decimal::TWO),
        _ => None,
    }
}
