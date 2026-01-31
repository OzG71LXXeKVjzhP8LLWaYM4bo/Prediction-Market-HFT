use chrono::Utc;
use pmhft_common::{
    KalshiMarketRef, MarketCategory, MatchedPair, PolymarketMarketRef, Result,
};
use pmhft_dome_client::rest::matching::DomeMatchingClient;
use pmhft_dome_client::types::DomeMatchEntry;
use tracing::info;

/// Discovers cross-platform pairs for sports markets using Dome's matching API.
pub struct DomeSportsMatcher {
    dome_client: DomeMatchingClient,
}

impl DomeSportsMatcher {
    pub fn new(dome_client: DomeMatchingClient) -> Self {
        Self { dome_client }
    }

    /// Fetch matched sports pairs for a given sport and date.
    pub async fn fetch_matches(
        &self,
        sport: &str,
        date: Option<&str>,
    ) -> Result<Vec<MatchedPair>> {
        let response = self.dome_client.get_sports_matches(sport, date).await?;
        let mut pairs = Vec::new();

        for entry in &response.markets {
            if let Some(pair) = self.convert_entry(entry) {
                pairs.push(pair);
            }
        }

        info!(
            sport = sport,
            found = pairs.len(),
            raw = response.markets.len(),
            "Dome sports matching complete"
        );

        Ok(pairs)
    }

    fn convert_entry(&self, entry: &DomeMatchEntry) -> Option<MatchedPair> {
        let poly_slug = entry.polymarket_slug.as_ref()?;
        let kalshi_ticker = entry.kalshi_market_ticker.as_ref()?;

        let pair_id = format!(
            "dome-{}-{}",
            poly_slug,
            kalshi_ticker.to_lowercase()
        );

        Some(MatchedPair {
            pair_id,
            polymarket: PolymarketMarketRef {
                market_slug: poly_slug.clone(),
                condition_id: entry.polymarket_condition_id.clone().unwrap_or_default(),
                token_ids: entry.polymarket_token_ids.clone(),
                question: entry.polymarket_question.clone().unwrap_or_default(),
            },
            kalshi: KalshiMarketRef {
                event_ticker: entry.kalshi_event_ticker.clone().unwrap_or_default(),
                market_tickers: vec![kalshi_ticker.clone()],
                title: entry.kalshi_title.clone().unwrap_or_default(),
            },
            match_confidence: 1.0, // Dome's matches are exact.
            category: MarketCategory::Sports,
            discovered_at: Utc::now(),
            last_verified: Utc::now(),
        })
    }
}
