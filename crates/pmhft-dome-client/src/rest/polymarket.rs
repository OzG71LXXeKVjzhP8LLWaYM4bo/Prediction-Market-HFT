use crate::rest::rate_limit::RateLimiter;
use crate::types::*;
use pmhft_common::{PmhftError, Result};
use reqwest::Client;
use std::time::Duration;

pub struct DomePolymarketClient {
    client: Client,
    base_url: String,
    api_key: String,
    timeout: Duration,
    rate_limiter: RateLimiter,
}

impl DomePolymarketClient {
    pub fn new(base_url: &str, api_key: &str, rate_limit_per_sec: u32, timeout_ms: u64) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
            timeout: Duration::from_millis(timeout_ms),
            rate_limiter: RateLimiter::new(rate_limit_per_sec),
        }
    }

    /// GET /polymarket/markets — search/list Polymarket markets.
    pub async fn get_markets(&self, params: &MarketSearchParams) -> Result<Vec<DomeMarket>> {
        self.rate_limiter.acquire().await;
        let resp = self
            .client
            .get(format!("{}/polymarket/markets", self.base_url))
            .bearer_auth(&self.api_key)
            .query(params)
            .timeout(self.timeout)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(PmhftError::DomeApi {
                status,
                message: body,
            });
        }

        Ok(resp.json().await?)
    }

    /// GET /polymarket/market-price/{token_id} — get current price.
    pub async fn get_market_price(&self, token_id: &str) -> Result<DomeMarketPrice> {
        self.rate_limiter.acquire().await;
        let resp = self
            .client
            .get(format!(
                "{}/polymarket/market-price/{}",
                self.base_url, token_id
            ))
            .bearer_auth(&self.api_key)
            .timeout(self.timeout)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(PmhftError::DomeApi {
                status,
                message: body,
            });
        }

        Ok(resp.json().await?)
    }

    /// GET /polymarket/orderbooks — get orderbook snapshot.
    pub async fn get_orderbooks(
        &self,
        token_id: &str,
        start_time: Option<u64>,
    ) -> Result<DomeOrderbookResponse> {
        self.rate_limiter.acquire().await;
        let mut req = self
            .client
            .get(format!("{}/polymarket/orderbooks", self.base_url))
            .bearer_auth(&self.api_key)
            .query(&[("token_id", token_id)]);

        if let Some(st) = start_time {
            req = req.query(&[("start_time", st.to_string())]);
        }

        let resp = req.timeout(self.timeout).send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(PmhftError::DomeApi {
                status,
                message: body,
            });
        }

        Ok(resp.json().await?)
    }

    /// POST /polymarket/placeOrder — JSON-RPC 2.0 order placement.
    pub async fn place_order(
        &self,
        request: &DomePlaceOrderRequest,
    ) -> Result<DomePlaceOrderResponse> {
        self.rate_limiter.acquire().await;
        let resp = self
            .client
            .post(format!("{}/polymarket/placeOrder", self.base_url))
            .bearer_auth(&self.api_key)
            .json(request)
            .timeout(self.timeout)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(PmhftError::DomeApi {
                status,
                message: body,
            });
        }

        Ok(resp.json().await?)
    }
}
