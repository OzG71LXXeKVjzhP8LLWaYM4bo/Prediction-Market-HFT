use serde::{Deserialize, Serialize};

// ── REST API Types ──

#[derive(Debug, Serialize)]
pub struct KalshiCreateOrderRequest {
    pub ticker: String,
    pub side: String,
    pub action: String,
    pub client_order_id: String,
    pub count: u32,
    #[serde(rename = "type")]
    pub order_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yes_price: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_price: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_in_force: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_only: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct KalshiCreateOrderResponse {
    pub order: KalshiOrder,
}

#[derive(Debug, Deserialize)]
pub struct KalshiOrder {
    pub order_id: String,
    pub client_order_id: Option<String>,
    pub ticker: Option<String>,
    pub side: Option<String>,
    pub action: Option<String>,
    pub status: Option<String>,
    pub yes_price: Option<u32>,
    pub no_price: Option<u32>,
    #[serde(default)]
    pub fill_count: u32,
    #[serde(default)]
    pub remaining_count: u32,
    #[serde(default)]
    pub initial_count: u32,
    pub taker_fees: Option<u32>,
    pub maker_fees: Option<u32>,
    pub created_time: Option<String>,
    pub last_update_time: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct KalshiCancelOrderResponse {
    pub order: Option<KalshiOrder>,
    pub reduced_by: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct KalshiMarket {
    pub ticker: Option<String>,
    pub event_ticker: Option<String>,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub status: Option<String>,
    pub yes_bid: Option<u32>,
    pub yes_ask: Option<u32>,
    pub no_bid: Option<u32>,
    pub no_ask: Option<u32>,
    pub volume: Option<u64>,
    pub open_interest: Option<u64>,
    pub close_time: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct KalshiMarketsResponse {
    #[serde(default)]
    pub markets: Vec<KalshiMarket>,
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct KalshiOrderbookResponse {
    pub orderbook: Option<KalshiOrderbook>,
}

#[derive(Debug, Deserialize)]
pub struct KalshiOrderbook {
    pub ticker: Option<String>,
    #[serde(default)]
    pub yes: Vec<Vec<u64>>,
    #[serde(default)]
    pub no: Vec<Vec<u64>>,
}
