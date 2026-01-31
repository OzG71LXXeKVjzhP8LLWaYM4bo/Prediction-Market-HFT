use chrono::Utc;
use pmhft_common::{KalshiMarketRef, MarketCategory, MatchedPair, PolymarketMarketRef};
use pmhft_dome_client::types::{DomeKalshiMarket, DomeMarket};
use strsim::jaro_winkler;
use tracing::{debug, info};

/// Fuzzy matcher for cross-platform pair discovery across all market categories.
///
/// Uses Jaro-Winkler similarity on normalized market titles to find matches
/// between Polymarket and Kalshi markets that Dome's API doesn't cover.
pub struct FuzzyMatcher {
    similarity_threshold: f64,
}

impl FuzzyMatcher {
    pub fn new(threshold: f64) -> Self {
        Self {
            similarity_threshold: threshold,
        }
    }

    /// Match Polymarket markets against Kalshi markets using fuzzy string comparison.
    ///
    /// Returns pairs sorted by confidence (highest first).
    pub fn find_matches(
        &self,
        poly_markets: &[DomeMarket],
        kalshi_markets: &[DomeKalshiMarket],
    ) -> Vec<MatchedPair> {
        let mut pairs = Vec::new();
        let mut used_kalshi: std::collections::HashSet<usize> = std::collections::HashSet::new();

        // Pre-normalize all titles.
        let poly_normalized: Vec<(usize, String)> = poly_markets
            .iter()
            .enumerate()
            .filter_map(|(i, m)| {
                m.question
                    .as_ref()
                    .map(|q| (i, normalize_title(q)))
            })
            .collect();

        let kalshi_normalized: Vec<(usize, String)> = kalshi_markets
            .iter()
            .enumerate()
            .filter_map(|(i, m)| {
                m.title
                    .as_ref()
                    .map(|t| (i, normalize_title(t)))
            })
            .collect();

        // O(n*m) comparison — acceptable for market lists (typically < 10,000 each).
        for &(pi, ref poly_norm) in &poly_normalized {
            let mut best_score = 0.0;
            let mut best_kalshi_idx: Option<usize> = None;

            for &(ki, ref kalshi_norm) in &kalshi_normalized {
                if used_kalshi.contains(&ki) {
                    continue;
                }

                let score = compute_match_score(poly_norm, kalshi_norm);

                if score > best_score && score >= self.similarity_threshold {
                    best_score = score;
                    best_kalshi_idx = Some(ki);
                }
            }

            if let Some(ki) = best_kalshi_idx {
                used_kalshi.insert(ki);

                let poly_market = &poly_markets[pi];
                let kalshi_market = &kalshi_markets[ki];

                let pair_id = format!(
                    "fuzzy-{}-{}",
                    poly_market
                        .market_slug
                        .as_deref()
                        .unwrap_or("unknown"),
                    kalshi_market
                        .ticker
                        .as_deref()
                        .unwrap_or("unknown")
                        .to_lowercase()
                );

                let category = infer_category(
                    poly_market.question.as_deref().unwrap_or(""),
                    &poly_market.tags,
                );

                let pair = MatchedPair {
                    pair_id,
                    polymarket: PolymarketMarketRef {
                        market_slug: poly_market
                            .market_slug
                            .clone()
                            .unwrap_or_default(),
                        condition_id: poly_market
                            .condition_id
                            .clone()
                            .unwrap_or_default(),
                        token_ids: poly_market
                            .tokens
                            .iter()
                            .filter_map(|t| t.token_id.clone())
                            .collect(),
                        question: poly_market.question.clone().unwrap_or_default(),
                    },
                    kalshi: KalshiMarketRef {
                        event_ticker: kalshi_market
                            .event_ticker
                            .clone()
                            .unwrap_or_default(),
                        market_tickers: kalshi_market
                            .ticker
                            .as_ref()
                            .map(|t| vec![t.clone()])
                            .unwrap_or_default(),
                        title: kalshi_market.title.clone().unwrap_or_default(),
                    },
                    match_confidence: best_score,
                    category,
                    discovered_at: Utc::now(),
                    last_verified: Utc::now(),
                };

                debug!(
                    pair_id = %pair.pair_id,
                    confidence = best_score,
                    poly_q = poly_market.question.as_deref().unwrap_or(""),
                    kalshi_t = kalshi_market.title.as_deref().unwrap_or(""),
                    "Fuzzy match found"
                );

                pairs.push(pair);
            }
        }

        // Sort by confidence descending.
        pairs.sort_by(|a, b| {
            b.match_confidence
                .partial_cmp(&a.match_confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        info!(
            total_matches = pairs.len(),
            threshold = self.similarity_threshold,
            "Fuzzy matching complete"
        );

        pairs
    }
}

/// Normalize a market title for comparison:
/// lowercase, strip punctuation, remove common filler words.
fn normalize_title(title: &str) -> String {
    let stopwords = [
        "will", "the", "be", "a", "an", "of", "in", "to", "on", "by", "at", "for", "is", "it",
        "or", "and", "vs", "versus", "over", "under", "above", "below", "before", "after",
        "this", "that", "what", "which",
    ];

    let lowered = title.to_lowercase();
    let cleaned: String = lowered
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' { c } else { ' ' })
        .collect();

    cleaned
        .split_whitespace()
        .filter(|w| !stopwords.contains(w))
        .collect::<Vec<&str>>()
        .join(" ")
}

/// Compute a match score between two normalized titles.
/// Combines Jaro-Winkler similarity with keyword overlap.
fn compute_match_score(a: &str, b: &str) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }

    // Jaro-Winkler string similarity (0.0 .. 1.0).
    let jw = jaro_winkler(a, b);

    // Keyword overlap: what fraction of words in the shorter string appear in the longer.
    let words_a: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let words_b: std::collections::HashSet<&str> = b.split_whitespace().collect();
    let intersection = words_a.intersection(&words_b).count() as f64;
    let min_len = words_a.len().min(words_b.len()) as f64;
    let overlap = if min_len > 0.0 {
        intersection / min_len
    } else {
        0.0
    };

    // Weighted combination: 60% Jaro-Winkler, 40% keyword overlap.
    0.6 * jw + 0.4 * overlap
}

/// Infer market category from title and tags.
fn infer_category(question: &str, tags: &[String]) -> MarketCategory {
    let text = question.to_lowercase();
    let tag_text: String = tags.iter().map(|t| t.to_lowercase()).collect::<Vec<_>>().join(" ");
    let combined = format!("{} {}", text, tag_text);

    if combined.contains("bitcoin")
        || combined.contains("ethereum")
        || combined.contains("crypto")
        || combined.contains("btc")
        || combined.contains("eth")
    {
        MarketCategory::Crypto
    } else if combined.contains("president")
        || combined.contains("election")
        || combined.contains("congress")
        || combined.contains("senate")
        || combined.contains("governor")
        || combined.contains("politics")
        || combined.contains("trump")
        || combined.contains("biden")
    {
        MarketCategory::Politics
    } else if combined.contains("nfl")
        || combined.contains("nba")
        || combined.contains("mlb")
        || combined.contains("nhl")
        || combined.contains("super bowl")
        || combined.contains("world series")
        || combined.contains("sports")
    {
        MarketCategory::Sports
    } else if combined.contains("weather")
        || combined.contains("temperature")
        || combined.contains("hurricane")
        || combined.contains("tornado")
    {
        MarketCategory::Weather
    } else if combined.contains("gdp")
        || combined.contains("inflation")
        || combined.contains("fed")
        || combined.contains("interest rate")
        || combined.contains("unemployment")
    {
        MarketCategory::Economics
    } else if combined.contains("oscar")
        || combined.contains("grammy")
        || combined.contains("emmy")
        || combined.contains("entertainment")
    {
        MarketCategory::Entertainment
    } else {
        MarketCategory::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_title() {
        assert_eq!(
            normalize_title("Will Bitcoin be above $100,000 on January 31?"),
            "bitcoin 100 000 january 31"
        );
    }

    #[test]
    fn test_match_score_identical() {
        let a = normalize_title("Will Bitcoin reach $100k?");
        let b = normalize_title("Will Bitcoin reach $100k?");
        let score = compute_match_score(&a, &b);
        assert!(score > 0.99);
    }

    #[test]
    fn test_match_score_similar() {
        let a = normalize_title("Will Bitcoin be above $100,000 by January 2026?");
        let b = normalize_title("Bitcoin above $100,000 January 2026");
        let score = compute_match_score(&a, &b);
        assert!(score > 0.85);
    }

    #[test]
    fn test_match_score_different() {
        let a = normalize_title("Will it rain in New York tomorrow?");
        let b = normalize_title("NFL Super Bowl winner 2026");
        let score = compute_match_score(&a, &b);
        assert!(score < 0.5);
    }

    #[test]
    fn test_infer_category() {
        assert_eq!(
            infer_category("Will Bitcoin hit $100k?", &[]),
            MarketCategory::Crypto
        );
        assert_eq!(
            infer_category("Will Trump win the election?", &[]),
            MarketCategory::Politics
        );
        assert_eq!(
            infer_category("Who wins the Super Bowl?", &["sports".into()]),
            MarketCategory::Sports
        );
    }
}
