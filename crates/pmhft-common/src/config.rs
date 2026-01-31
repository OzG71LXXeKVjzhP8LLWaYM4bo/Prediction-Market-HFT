use rust_decimal::Decimal;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub dome: DomeConfig,
    pub kalshi: KalshiConfig,
    pub polymarket: PolymarketConfig,
    pub matching: MatchingConfig,
    pub strategy: StrategyConfig,
    pub risk: RiskConfig,
    pub execution: ExecutionConfig,
    pub telemetry: TelemetryConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DomeConfig {
    pub api_key: String,
    pub base_url: String,
    pub ws_url: String,
    /// Free tier = 1 req/s, Dev = 100, Pro = 300.
    pub rate_limit_per_sec: u32,
    pub request_timeout_ms: u64,
}

impl Default for DomeConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.domeapi.io/v1".into(),
            ws_url: "wss://ws.domeapi.io".into(),
            rate_limit_per_sec: 1,
            request_timeout_ms: 5000,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct KalshiConfig {
    pub api_key_id: String,
    pub private_key_path: String,
    pub rest_base_url: String,
    /// FIX 4.4 gateway host.
    pub fix_host: String,
    /// FIX 4.4 gateway port.
    pub fix_port: u16,
    /// FIX SenderCompID assigned by Kalshi.
    pub fix_sender_comp_id: String,
    /// FIX TargetCompID (Kalshi's identifier).
    pub fix_target_comp_id: String,
}

impl Default for KalshiConfig {
    fn default() -> Self {
        Self {
            api_key_id: String::new(),
            private_key_path: String::new(),
            rest_base_url: "https://demo-api.kalshi.co/trade-api/v2".into(),
            fix_host: String::new(),
            fix_port: 0,
            fix_sender_comp_id: String::new(),
            fix_target_comp_id: String::new(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct PolymarketConfig {
    pub wallet_private_key: String,
    pub chain_id: u64,
    pub exchange_address: String,
    pub api_key: String,
    pub api_secret: String,
    pub api_passphrase: String,
}

impl Default for PolymarketConfig {
    fn default() -> Self {
        Self {
            wallet_private_key: String::new(),
            chain_id: 137,
            exchange_address: "0x4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E".into(),
            api_key: String::new(),
            api_secret: String::new(),
            api_passphrase: String::new(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct MatchingConfig {
    /// How often to refresh sports pairs from Dome (seconds).
    pub sports_refresh_interval_sec: u64,
    /// How often to run fuzzy matching for non-sports markets (seconds).
    pub fuzzy_refresh_interval_sec: u64,
    /// Jaro-Winkler similarity threshold for fuzzy matching.
    pub fuzzy_similarity_threshold: f64,
    /// Minimum daily volume (USD) for a market to be tradeable.
    pub min_volume_usd: f64,
    /// Maximum bid-ask spread (%) to filter out illiquid markets.
    pub max_spread_pct: f64,
    /// Exclude markets expiring within this many hours.
    pub exclude_near_expiry_hours: u64,
}

impl Default for MatchingConfig {
    fn default() -> Self {
        Self {
            sports_refresh_interval_sec: 900,
            fuzzy_refresh_interval_sec: 1800,
            fuzzy_similarity_threshold: 0.85,
            min_volume_usd: 1000.0,
            max_spread_pct: 10.0,
            exclude_near_expiry_hours: 2,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct StrategyConfig {
    /// Minimum edge (in basis points) required to trigger a trade.
    pub min_edge_bps: Decimal,
    /// Z-score threshold to enter a statistical arb position.
    pub z_score_entry: Decimal,
    /// Z-score threshold to exit a statistical arb position.
    pub z_score_exit: Decimal,
    /// Number of ticks in the rolling spread window.
    pub spread_window: usize,
    /// Maximum half-life (seconds) for OU mean-reversion.
    pub max_half_life_sec: f64,
    /// Reject quotes older than this (ms).
    pub max_quote_age_ms: u64,
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            min_edge_bps: Decimal::new(50, 0),
            z_score_entry: Decimal::new(20, 1),
            z_score_exit: Decimal::new(5, 1),
            spread_window: 100,
            max_half_life_sec: 3600.0,
            max_quote_age_ms: 5000,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct RiskConfig {
    pub max_position_per_market: Decimal,
    pub max_gross_exposure_usd: Decimal,
    pub max_order_notional_usd: Decimal,
    pub max_daily_loss_usd: Decimal,
    pub max_open_orders: u32,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            max_position_per_market: Decimal::new(500, 0),
            max_gross_exposure_usd: Decimal::new(10000, 0),
            max_order_notional_usd: Decimal::new(1000, 0),
            max_daily_loss_usd: Decimal::new(-500, 0),
            max_open_orders: 20,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct ExecutionConfig {
    /// Whether to actually submit orders (false = paper trading).
    pub live_trading: bool,
    /// Default order type: "FOK", "FAK", or "GTC".
    pub default_order_type: String,
    /// Whether to use Kalshi FIX (true) or REST fallback (false).
    pub use_fix_for_kalshi: bool,
    /// Whether to route Polymarket orders through Dome placeOrder.
    pub use_dome_for_polymarket: bool,
    /// Maximum slippage tolerance (bps) from signal price.
    pub max_slippage_bps: Decimal,
    /// Timeout waiting for fill confirmation (ms).
    pub fill_timeout_ms: u64,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            live_trading: false,
            default_order_type: "FOK".into(),
            use_fix_for_kalshi: true,
            use_dome_for_polymarket: true,
            max_slippage_bps: Decimal::new(20, 0),
            fill_timeout_ms: 3000,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct TelemetryConfig {
    pub log_level: String,
    pub prometheus_port: u16,
    pub enable_json_logs: bool,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            log_level: "info".into(),
            prometheus_port: 9090,
            enable_json_logs: false,
        }
    }
}
