use serde::{Deserialize, Serialize};

// ── Polymarket Endpoints ──

#[derive(Debug, Serialize)]
pub struct MarketSearchParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DomeMarket {
    pub market_slug: Option<String>,
    pub question: Option<String>,
    pub condition_id: Option<String>,
    #[serde(default)]
    pub tokens: Vec<DomeToken>,
    pub end_date_iso: Option<String>,
    pub active: Option<bool>,
    pub closed: Option<bool>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DomeToken {
    pub token_id: Option<String>,
    pub outcome: Option<String>,
    pub price: Option<f64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DomeMarketPrice {
    pub token_id: Option<String>,
    pub price: Option<f64>,
    pub timestamp: Option<u64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DomeOrderbookResponse {
    #[serde(default)]
    pub bids: Vec<DomeOrderbookLevel>,
    #[serde(default)]
    pub asks: Vec<DomeOrderbookLevel>,
    pub timestamp: Option<u64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DomeOrderbookLevel {
    pub price: Option<f64>,
    pub size: Option<f64>,
}

// ── Kalshi Endpoints (via Dome) ──

#[derive(Debug, Deserialize, Clone)]
pub struct DomeKalshiMarket {
    pub ticker: Option<String>,
    pub event_ticker: Option<String>,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub status: Option<String>,
    pub yes_bid: Option<f64>,
    pub yes_ask: Option<f64>,
    pub no_bid: Option<f64>,
    pub no_ask: Option<f64>,
    pub volume: Option<u64>,
    pub open_interest: Option<u64>,
    pub close_time: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DomeKalshiMarketPrice {
    pub ticker: Option<String>,
    pub yes_price: Option<f64>,
    pub no_price: Option<f64>,
    pub timestamp: Option<u64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DomeKalshiOrderbook {
    pub ticker: Option<String>,
    #[serde(default)]
    pub yes_bids: Vec<DomeOrderbookLevel>,
    #[serde(default)]
    pub yes_asks: Vec<DomeOrderbookLevel>,
    #[serde(default)]
    pub no_bids: Vec<DomeOrderbookLevel>,
    #[serde(default)]
    pub no_asks: Vec<DomeOrderbookLevel>,
    pub timestamp: Option<u64>,
}

// ── Matching Markets ──

#[derive(Debug, Deserialize, Clone)]
pub struct DomeMatchingResponse {
    #[serde(default)]
    pub markets: Vec<DomeMatchEntry>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DomeMatchEntry {
    pub sport: Option<String>,
    pub date: Option<String>,
    pub polymarket_slug: Option<String>,
    pub polymarket_question: Option<String>,
    pub polymarket_condition_id: Option<String>,
    #[serde(default)]
    pub polymarket_token_ids: Vec<String>,
    pub kalshi_event_ticker: Option<String>,
    pub kalshi_market_ticker: Option<String>,
    pub kalshi_title: Option<String>,
}

// ── Polymarket Order Placement (JSON-RPC 2.0) ──

#[derive(Debug, Serialize)]
pub struct DomePlaceOrderRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: DomePlaceOrderParams,
    pub id: u64,
}

#[derive(Debug, Serialize)]
pub struct DomePlaceOrderParams {
    pub token_id: String,
    pub side: String,
    pub price: String,
    pub size: String,
    pub order_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_passphrase: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DomePlaceOrderResponse {
    pub jsonrpc: Option<String>,
    pub result: Option<DomePlaceOrderResult>,
    pub error: Option<DomeJsonRpcError>,
    pub id: Option<u64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DomePlaceOrderResult {
    pub order_id: Option<String>,
    pub status: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DomeJsonRpcError {
    pub code: Option<i32>,
    pub message: Option<String>,
}

// ── WebSocket Messages ──

#[derive(Debug, Deserialize, Clone)]
pub struct DomeWsMessage {
    #[serde(rename = "type")]
    pub msg_type: Option<String>,
    pub data: Option<DomeWsOrderEvent>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DomeWsOrderEvent {
    pub market_slug: Option<String>,
    pub token_id: Option<String>,
    pub side: Option<String>,
    pub price: Option<f64>,
    pub shares_normalized: Option<f64>,
    pub timestamp: Option<u64>,
    pub order_hash: Option<String>,
    pub taker: Option<String>,
    pub maker: Option<String>,
}

// ── WebSocket Subscription ──

#[derive(Debug, Serialize)]
pub struct DomeWsSubscription {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub condition_ids: Vec<String>,
}
