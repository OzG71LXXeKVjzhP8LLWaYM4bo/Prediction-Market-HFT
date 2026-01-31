use crate::dome_matcher::DomeSportsMatcher;
use crate::fuzzy_matcher::FuzzyMatcher;
use crate::registry::PairRegistry;
use pmhft_common::config::{DomeConfig, MatchingConfig};
use pmhft_common::Result;
use pmhft_dome_client::rest::{DomeKalshiClient, DomeMatchingClient, DomePolymarketClient};
use pmhft_dome_client::types::MarketSearchParams;
use std::sync::Arc;
use tokio::time::{self, Duration};
use tracing::{info, warn};

/// Periodically refreshes the pair registry from both Dome sports matching
/// and custom fuzzy matching.
pub struct PairReconciler {
    registry: Arc<PairRegistry>,
    sports_matcher: DomeSportsMatcher,
    fuzzy_matcher: FuzzyMatcher,
    poly_client: DomePolymarketClient,
    kalshi_client: DomeKalshiClient,
    config: MatchingConfig,
}

impl PairReconciler {
    pub fn new(
        registry: Arc<PairRegistry>,
        dome_config: &DomeConfig,
        matching_config: &MatchingConfig,
    ) -> Self {
        let sports_matcher = DomeSportsMatcher::new(DomeMatchingClient::new(
            &dome_config.base_url,
            &dome_config.api_key,
            dome_config.rate_limit_per_sec,
            dome_config.request_timeout_ms,
        ));

        let fuzzy_matcher = FuzzyMatcher::new(matching_config.fuzzy_similarity_threshold);

        let poly_client = DomePolymarketClient::new(
            &dome_config.base_url,
            &dome_config.api_key,
            dome_config.rate_limit_per_sec,
            dome_config.request_timeout_ms,
        );

        let kalshi_client = DomeKalshiClient::new(
            &dome_config.base_url,
            &dome_config.api_key,
            dome_config.rate_limit_per_sec,
            dome_config.request_timeout_ms,
        );

        Self {
            registry,
            sports_matcher,
            fuzzy_matcher,
            poly_client,
            kalshi_client,
            config: matching_config.clone(),
        }
    }

    /// Run both sports and fuzzy matching once.
    pub async fn refresh_once(&self) -> Result<()> {
        self.refresh_sports().await;
        self.refresh_fuzzy().await;
        Ok(())
    }

    /// Run the periodic reconciliation loop.
    pub async fn run_loop(&self) {
        let sports_interval = Duration::from_secs(self.config.sports_refresh_interval_sec);
        let fuzzy_interval = Duration::from_secs(self.config.fuzzy_refresh_interval_sec);

        let mut sports_timer = time::interval(sports_interval);
        let mut fuzzy_timer = time::interval(fuzzy_interval);

        loop {
            tokio::select! {
                _ = sports_timer.tick() => {
                    self.refresh_sports().await;
                }
                _ = fuzzy_timer.tick() => {
                    self.refresh_fuzzy().await;
                }
            }
        }
    }

    async fn refresh_sports(&self) {
        let sports = ["nfl", "nba", "mlb", "nhl", "mls", "ncaaf", "ncaab"];
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

        for sport in &sports {
            match self.sports_matcher.fetch_matches(sport, Some(&today)).await {
                Ok(pairs) => {
                    for pair in pairs {
                        self.registry.upsert(pair);
                    }
                }
                Err(e) => {
                    warn!(sport = sport, error = %e, "Sports matching failed");
                }
            }
        }

        info!(
            total_pairs = self.registry.len(),
            "Sports pair refresh complete"
        );
    }

    async fn refresh_fuzzy(&self) {
        // Fetch all open Polymarket markets.
        let poly_markets = match self
            .poly_client
            .get_markets(&MarketSearchParams {
                slug: None,
                search: None,
                tag: None,
                start_date: None,
                end_date: None,
            })
            .await
        {
            Ok(m) => m,
            Err(e) => {
                warn!(error = %e, "Failed to fetch Polymarket markets for fuzzy matching");
                return;
            }
        };

        // Fetch all open Kalshi markets.
        let kalshi_markets = match self.kalshi_client.get_markets(None, Some("open")).await {
            Ok(m) => m,
            Err(e) => {
                warn!(error = %e, "Failed to fetch Kalshi markets for fuzzy matching");
                return;
            }
        };

        // Run fuzzy matching.
        let pairs = self
            .fuzzy_matcher
            .find_matches(&poly_markets, &kalshi_markets);

        for pair in pairs {
            // Only auto-register high-confidence matches.
            if pair.match_confidence >= self.config.fuzzy_similarity_threshold {
                self.registry.upsert(pair);
            }
        }

        info!(
            total_pairs = self.registry.len(),
            "Fuzzy pair refresh complete"
        );
    }
}
