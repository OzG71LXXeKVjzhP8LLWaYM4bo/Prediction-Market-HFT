use crate::types::Platform;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PmhftError {
    #[error("Dome API error: {status} - {message}")]
    DomeApi { status: u16, message: String },

    #[error("Kalshi API error: {status} - {message}")]
    KalshiApi { status: u16, message: String },

    #[error("Kalshi FIX error: {0}")]
    KalshiFix(String),

    #[error("WebSocket error: {0}")]
    WebSocket(String),

    #[error("Rate limit exceeded for {platform}, retry after {retry_after_ms}ms")]
    RateLimited {
        platform: Platform,
        retry_after_ms: u64,
    },

    #[error("EIP-712 signing error: {0}")]
    Eip712Signing(String),

    #[error("RSA signing error: {0}")]
    RsaSigning(String),

    #[error("Order rejected: {reason}")]
    OrderRejected { reason: String },

    #[error("Risk limit breached: {0}")]
    RiskLimitBreached(String),

    #[error("Market not found: {0}")]
    MarketNotFound(String),

    #[error("Pair not matched: {0}")]
    PairNotMatched(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("FIX session error: {0}")]
    FixSession(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, PmhftError>;
