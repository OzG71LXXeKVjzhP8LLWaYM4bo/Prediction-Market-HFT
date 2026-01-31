use crate::rest::rate_limit::RateLimiter;
use crate::types::*;
use pmhft_common::{PmhftError, Result};
use reqwest::Client;
use std::time::Duration;

pub struct DomeMatchingClient {
    client: Client,
    base_url: String,
    api_key: String,
    timeout: Duration,
    rate_limiter: RateLimiter,
}

impl DomeMatchingClient {
    pub fn new(base_url: &str, api_key: &str, rate_limit_per_sec: u32, timeout_ms: u64) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
            timeout: Duration::from_millis(timeout_ms),
            rate_limiter: RateLimiter::new(rate_limit_per_sec),
        }
    }

    /// GET /matching-markets/sports — get cross-platform sports matches.
    pub async fn get_sports_matches(
        &self,
        sport: &str,
        date: Option<&str>,
    ) -> Result<DomeMatchingResponse> {
        self.rate_limiter.acquire().await;
        let mut req = self
            .client
            .get(format!("{}/matching-markets/sports", self.base_url))
            .bearer_auth(&self.api_key)
            .query(&[("sport", sport)]);

        if let Some(d) = date {
            req = req.query(&[("date", d)]);
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
