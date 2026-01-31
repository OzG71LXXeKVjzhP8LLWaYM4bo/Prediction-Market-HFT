use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::time::{self, Duration};

/// Token-bucket rate limiter for Dome API.
///
/// Free tier: 1 req/s (burst of 1).
/// Dev tier: 100 req/s.
/// Pro tier: 300 req/s.
pub struct RateLimiter {
    semaphore: Arc<Semaphore>,
    _refill_handle: tokio::task::JoinHandle<()>,
}

impl RateLimiter {
    pub fn new(tokens_per_sec: u32) -> Self {
        let tokens = tokens_per_sec.max(1) as usize;
        let semaphore = Arc::new(Semaphore::new(tokens));
        let sem_clone = semaphore.clone();

        let refill_handle = tokio::spawn(async move {
            let interval = Duration::from_secs_f64(1.0 / tokens as f64);
            let mut ticker = time::interval(interval);
            // First tick completes immediately; skip it.
            ticker.tick().await;

            loop {
                ticker.tick().await;
                if sem_clone.available_permits() < tokens {
                    sem_clone.add_permits(1);
                }
            }
        });

        Self {
            semaphore,
            _refill_handle: refill_handle,
        }
    }

    /// Acquire a token. Blocks until a token is available.
    pub async fn acquire(&self) {
        let permit = self.semaphore.acquire().await.expect("semaphore closed");
        // Consume the permit (don't hold it).
        permit.forget();
    }
}
