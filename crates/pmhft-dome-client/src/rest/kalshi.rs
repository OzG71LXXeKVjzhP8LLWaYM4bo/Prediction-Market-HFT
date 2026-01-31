use crate::rest::rate_limit::RateLimiter;
use crate::types::*;
use pmhft_common::{PmhftError, Result};
use reqwest::Client;
use std::time::Duration;

pub struct DomeKalshiClient {
    client: Client,
    base_url: String,
    api_key: String,
    timeout: Duration,
    rate_limiter: RateLimiter,
}

impl DomeKalshiClient {
    pub fn new(base_url: &str, api_key: &str, rate_limit_per_sec: u32, timeout_ms: u64) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
            timeout: Duration::from_millis(timeout_ms),
            rate_limiter: RateLimiter::new(rate_limit_per_sec),
        }
    }

    /// GET /kalshi/markets — search/list Kalshi markets.
    pub async fn get_markets(
        &self,
        search: Option<&str>,
        status: Option<&str>,
    ) -> Result<Vec<DomeKalshiMarket>> {
        self.rate_limiter.acquire().await;
        let mut req = self
            .client
            .get(format!("{}/kalshi/markets", self.base_url))
            .bearer_auth(&self.api_key);

        if let Some(s) = search {
            req = req.query(&[("search", s)]);
        }
        if let Some(st) = status {
            req = req.query(&[("status", st)]);
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

    /// GET /kalshi/market-price/{ticker} — get current Kalshi prices.
    pub async fn get_market_price(&self, ticker: &str) -> Result<DomeKalshiMarketPrice> {
        self.rate_limiter.acquire().await;
        let resp = self
            .client
            .get(format!("{}/kalshi/market-price/{}", self.base_url, ticker))
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

    /// GET /kalshi/orderbooks — get orderbook snapshot for a ticker.
    pub async fn get_orderbook(
        &self,
        ticker: &str,
        start_time: Option<u64>,
    ) -> Result<DomeKalshiOrderbook> {
        self.rate_limiter.acquire().await;
        let mut req = self
            .client
            .get(format!("{}/kalshi/orderbooks", self.base_url))
            .bearer_auth(&self.api_key)
            .query(&[("ticker", ticker)]);

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
}
