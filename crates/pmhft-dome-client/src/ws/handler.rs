use crate::types::DomeWsOrderEvent;
use chrono::{DateTime, Utc};
use pmhft_common::{MarketId, NormalizedQuote, OutcomeSide, Platform, PriceLevel};
use rust_decimal::prelude::FromPrimitive;
use rust_decimal::Decimal;

/// Parse a Dome WebSocket order event into a NormalizedQuote update.
///
/// Dome WS events represent individual order actions (new orders, fills).
/// We extract the price and size as a point-in-time quote.
pub fn parse_order_event(event: &DomeWsOrderEvent) -> Option<NormalizedQuote> {
    let token_id = event.token_id.as_ref()?;
    let price_f = event.price?;
    let size_f = event.shares_normalized?;
    let side_str = event.side.as_deref()?;

    let price = Decimal::from_f64(price_f)?;
    let size = Decimal::from_f64(size_f)?;

    let timestamp: DateTime<Utc> = match event.timestamp {
        Some(ts) => {
            DateTime::from_timestamp_millis(ts as i64).unwrap_or_else(Utc::now)
        }
        None => Utc::now(),
    };

    // Dome reports side as "BUY" or "SELL" — a BUY order adds to the bid side,
    // a SELL order adds to the ask side.
    let (best_bid, best_ask) = match side_str {
        "BUY" | "buy" => (
            Some(PriceLevel { price, size }),
            None,
        ),
        "SELL" | "sell" => (
            None,
            Some(PriceLevel { price, size }),
        ),
        _ => return None,
    };

    Some(NormalizedQuote {
        market_id: MarketId {
            platform: Platform::Polymarket,
            id: token_id.clone(),
        },
        side: OutcomeSide::Yes, // Will be resolved by the market data layer
        best_bid,
        best_ask,
        mid_price: None,
        timestamp,
        sequence: event.timestamp.unwrap_or(0),
    })
}
