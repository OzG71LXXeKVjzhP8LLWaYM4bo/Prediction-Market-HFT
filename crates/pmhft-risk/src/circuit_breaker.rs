use std::sync::atomic::{AtomicBool, Ordering};
use tracing::error;

/// Emergency circuit breaker that halts all trading.
pub struct CircuitBreaker {
    active: AtomicBool,
}

impl CircuitBreaker {
    pub fn new() -> Self {
        Self {
            active: AtomicBool::new(false),
        }
    }

    /// Trip the circuit breaker — halts all new trading.
    pub fn trip(&self, reason: &str) {
        self.active.store(true, Ordering::SeqCst);
        error!(reason = reason, "CIRCUIT BREAKER TRIPPED — all trading halted");
    }

    /// Reset the circuit breaker (manual override).
    pub fn reset(&self) {
        self.active.store(false, Ordering::SeqCst);
        tracing::info!("Circuit breaker reset");
    }

    /// Check if the circuit breaker is active.
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}
