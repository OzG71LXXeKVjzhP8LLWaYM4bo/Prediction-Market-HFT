use dashmap::DashMap;
use pmhft_common::MatchedPair;
use std::sync::Arc;

/// Thread-safe registry of cross-platform matched pairs.
///
/// Provides O(1) lookups by pair_id, Polymarket token_id, or Kalshi ticker.
pub struct PairRegistry {
    /// pair_id -> MatchedPair.
    pairs: Arc<DashMap<String, MatchedPair>>,
    /// Polymarket token_id -> pair_id.
    poly_token_to_pair: Arc<DashMap<String, String>>,
    /// Kalshi market_ticker -> pair_id.
    kalshi_ticker_to_pair: Arc<DashMap<String, String>>,
}

impl PairRegistry {
    pub fn new() -> Self {
        Self {
            pairs: Arc::new(DashMap::new()),
            poly_token_to_pair: Arc::new(DashMap::new()),
            kalshi_ticker_to_pair: Arc::new(DashMap::new()),
        }
    }

    /// Insert or update a matched pair.
    pub fn upsert(&self, pair: MatchedPair) {
        let pair_id = pair.pair_id.clone();

        // Build reverse indexes.
        for tid in &pair.polymarket.token_ids {
            self.poly_token_to_pair
                .insert(tid.clone(), pair_id.clone());
        }
        for mticker in &pair.kalshi.market_tickers {
            self.kalshi_ticker_to_pair
                .insert(mticker.clone(), pair_id.clone());
        }

        self.pairs.insert(pair_id, pair);
    }

    /// Remove a pair and its reverse indexes.
    pub fn remove(&self, pair_id: &str) {
        if let Some((_, pair)) = self.pairs.remove(pair_id) {
            for tid in &pair.polymarket.token_ids {
                self.poly_token_to_pair.remove(tid);
            }
            for mticker in &pair.kalshi.market_tickers {
                self.kalshi_ticker_to_pair.remove(mticker);
            }
        }
    }

    /// Look up a pair by Polymarket token_id.
    pub fn get_by_poly_token(&self, token_id: &str) -> Option<MatchedPair> {
        self.poly_token_to_pair
            .get(token_id)
            .and_then(|pair_id| self.pairs.get(pair_id.value()).map(|p| p.clone()))
    }

    /// Look up a pair by Kalshi market_ticker.
    pub fn get_by_kalshi_ticker(&self, ticker: &str) -> Option<MatchedPair> {
        self.kalshi_ticker_to_pair
            .get(ticker)
            .and_then(|pair_id| self.pairs.get(pair_id.value()).map(|p| p.clone()))
    }

    /// Get a pair by pair_id.
    pub fn get(&self, pair_id: &str) -> Option<MatchedPair> {
        self.pairs.get(pair_id).map(|p| p.clone())
    }

    /// Get all active pairs.
    pub fn all_pairs(&self) -> Vec<MatchedPair> {
        self.pairs.iter().map(|entry| entry.value().clone()).collect()
    }

    /// Get all Polymarket condition_ids for active subscriptions.
    pub fn all_poly_condition_ids(&self) -> Vec<String> {
        self.pairs
            .iter()
            .map(|entry| entry.value().polymarket.condition_id.clone())
            .filter(|cid| !cid.is_empty())
            .collect()
    }

    /// Get all Kalshi market_tickers for active subscriptions.
    pub fn all_kalshi_tickers(&self) -> Vec<String> {
        self.pairs
            .iter()
            .flat_map(|entry| entry.value().kalshi.market_tickers.clone())
            .collect()
    }

    /// Number of active pairs.
    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }
}

impl Default for PairRegistry {
    fn default() -> Self {
        Self::new()
    }
}
