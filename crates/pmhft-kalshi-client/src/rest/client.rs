use crate::auth::KalshiAuth;
use crate::types::*;
use pmhft_common::{PmhftError, Result};
use reqwest::Client;
use std::time::Duration;

/// Kalshi REST API client (direct, not via Dome).
/// Used as fallback when FIX is unavailable, and for dev against demo-api.kalshi.co.
pub struct KalshiRestClient {
    client: Client,
    base_url: String,
    auth: KalshiAuth,
    timeout: Duration,
}

impl KalshiRestClient {
    pub fn new(base_url: &str, auth: KalshiAuth) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.to_string(),
            auth,
            timeout: Duration::from_secs(5),
        }
    }

    /// POST /trade-api/v2/portfolio/orders — create an order.
    pub async fn create_order(
        &self,
        req: &KalshiCreateOrderRequest,
    ) -> Result<KalshiCreateOrderResponse> {
        let path = "/trade-api/v2/portfolio/orders";
        let url = format!("{}{}", self.base_url, path);

        let builder = self.client.post(&url).json(req).timeout(self.timeout);
        let builder = self.auth.apply_headers(builder, "POST", path);

        let resp = builder.send().await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            Err(PmhftError::KalshiApi {
                status,
                message: body,
            })
        }
    }

    /// DELETE /trade-api/v2/portfolio/orders/{order_id} — cancel an order.
    pub async fn cancel_order(&self, order_id: &str) -> Result<KalshiCancelOrderResponse> {
        let path = format!("/trade-api/v2/portfolio/orders/{}", order_id);
        let url = format!("{}{}", self.base_url, path);

        let builder = self.client.delete(&url).timeout(self.timeout);
        let builder = self.auth.apply_headers(builder, "DELETE", &path);

        let resp = builder.send().await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            Err(PmhftError::KalshiApi {
                status,
                message: body,
            })
        }
    }

    /// GET /trade-api/v2/markets — list markets.
    pub async fn get_markets(
        &self,
        cursor: Option<&str>,
        limit: Option<u32>,
        status: Option<&str>,
    ) -> Result<KalshiMarketsResponse> {
        let path = "/trade-api/v2/markets";
        let url = format!("{}{}", self.base_url, path);

        let mut builder = self.client.get(&url).timeout(self.timeout);
        if let Some(c) = cursor {
            builder = builder.query(&[("cursor", c)]);
        }
        if let Some(l) = limit {
            builder = builder.query(&[("limit", l.to_string())]);
        }
        if let Some(s) = status {
            builder = builder.query(&[("status", s)]);
        }
        let builder = self.auth.apply_headers(builder, "GET", path);

        let resp = builder.send().await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            Err(PmhftError::KalshiApi {
                status,
                message: body,
            })
        }
    }

    /// GET /trade-api/v2/markets/{ticker}/orderbook — get orderbook.
    pub async fn get_orderbook(&self, ticker: &str) -> Result<KalshiOrderbookResponse> {
        let path = format!("/trade-api/v2/markets/{}/orderbook", ticker);
        let url = format!("{}{}", self.base_url, path);

        let builder = self.client.get(&url).timeout(self.timeout);
        let builder = self.auth.apply_headers(builder, "GET", &path);

        let resp = builder.send().await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            Err(PmhftError::KalshiApi {
                status,
                message: body,
            })
        }
    }
}
