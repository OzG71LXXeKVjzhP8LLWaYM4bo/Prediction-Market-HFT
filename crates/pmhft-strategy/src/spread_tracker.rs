use std::collections::VecDeque;

/// Tracks the cross-platform spread over time for statistical arbitrage.
///
/// Maintains a rolling window of (timestamp_ms, spread) observations
/// and computes z-score and OU half-life for mean-reversion signals.
pub struct SpreadTracker {
    history: VecDeque<(i64, f64)>,
    max_len: usize,
    mean: f64,
    variance: f64,
    count: usize,
    half_life_secs: Option<f64>,
}

impl SpreadTracker {
    pub fn new(window_size: usize) -> Self {
        Self {
            history: VecDeque::with_capacity(window_size),
            max_len: window_size,
            mean: 0.0,
            variance: 0.0,
            count: 0,
            half_life_secs: None,
        }
    }

    /// Add a new spread observation.
    pub fn update(&mut self, timestamp_ms: i64, spread: f64) {
        self.history.push_back((timestamp_ms, spread));
        if self.history.len() > self.max_len {
            self.history.pop_front();
        }
        self.recompute_stats();
    }

    /// Current z-score of the spread.
    pub fn zscore(&self, current_spread: f64) -> Option<f64> {
        if self.variance <= 0.0 || self.count < 30 {
            return None;
        }
        Some((current_spread - self.mean) / self.variance.sqrt())
    }

    /// Estimated half-life in seconds from OU model.
    pub fn half_life(&self) -> Option<f64> {
        self.half_life_secs
    }

    /// Current mean of the spread.
    pub fn mean(&self) -> f64 {
        self.mean
    }

    /// Current standard deviation of the spread.
    pub fn std_dev(&self) -> f64 {
        self.variance.sqrt()
    }

    /// Number of observations in the window.
    pub fn count(&self) -> usize {
        self.count
    }

    fn recompute_stats(&mut self) {
        let n = self.history.len();
        if n < 2 {
            self.count = n;
            return;
        }

        let n_f = n as f64;
        let sum: f64 = self.history.iter().map(|(_, s)| s).sum();
        self.mean = sum / n_f;
        self.variance = self
            .history
            .iter()
            .map(|(_, s)| (s - self.mean).powi(2))
            .sum::<f64>()
            / (n_f - 1.0);
        self.count = n;

        // Estimate OU half-life via AR(1) OLS when we have enough data.
        if n >= 30 {
            self.half_life_secs = self.estimate_half_life();
        }
    }

    /// Estimate half-life using AR(1) regression.
    ///
    /// Model: spread[t] = alpha + beta * spread[t-1] + epsilon
    /// half_life = -dt * ln(2) / ln(beta)
    fn estimate_half_life(&self) -> Option<f64> {
        let n = self.history.len() - 1;
        if n < 10 {
            return None;
        }

        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut sum_xy = 0.0;
        let mut sum_xx = 0.0;

        for i in 0..n {
            let x = self.history[i].1;
            let y = self.history[i + 1].1;
            sum_x += x;
            sum_y += y;
            sum_xy += x * y;
            sum_xx += x * x;
        }

        let n_f = n as f64;
        let denom = n_f * sum_xx - sum_x * sum_x;
        if denom.abs() < 1e-12 {
            return None;
        }

        let beta = (n_f * sum_xy - sum_x * sum_y) / denom;

        // beta must be in (0, 1) for mean-reversion.
        if beta <= 0.0 || beta >= 1.0 {
            return None;
        }

        // Average time step between observations.
        let total_time_ms = (self.history.back()?.0 - self.history.front()?.0) as f64;
        let dt_secs = total_time_ms / (self.history.len() as f64 * 1000.0);
        if dt_secs <= 0.0 {
            return None;
        }

        let half_life = -dt_secs * 2.0_f64.ln() / beta.ln();
        if half_life > 0.0 {
            Some(half_life)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zscore_insufficient_data() {
        let tracker = SpreadTracker::new(100);
        assert!(tracker.zscore(0.5).is_none());
    }

    #[test]
    fn test_zscore_calculation() {
        let mut tracker = SpreadTracker::new(100);
        // Feed a series of values with known mean and variance.
        for i in 0..50 {
            let spread = if i % 2 == 0 { 0.02 } else { -0.02 };
            tracker.update(i * 1000, spread);
        }

        // Mean should be ~0, std dev ~0.02.
        assert!(tracker.mean().abs() < 0.001);
        assert!((tracker.std_dev() - 0.02).abs() < 0.005);

        // A spread of 0.04 should have z-score ~2.
        let z = tracker.zscore(0.04).unwrap();
        assert!((z - 2.0).abs() < 0.5);
    }
}
