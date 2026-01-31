mod app;

use app::App;
use clap::Parser;
use pmhft_common::config::AppConfig;

#[derive(Parser)]
#[command(
    name = "pmhft",
    about = "Prediction Market HFT — Cross-platform statistical arbitrage (Polymarket x Kalshi)"
)]
struct Cli {
    /// Path to configuration file.
    #[arg(short, long, default_value = "config/default.toml")]
    config: String,

    /// Override: enable live trading (default: paper trading).
    #[arg(long)]
    live: bool,

    /// Override: log level (trace, debug, info, warn, error).
    #[arg(long)]
    log_level: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Load layered configuration: file + environment variables (PMHFT__*).
    let settings = config::Config::builder()
        .add_source(config::File::with_name(&cli.config).required(false))
        .add_source(config::Environment::with_prefix("PMHFT").separator("__"))
        .build()?;

    let mut app_config: AppConfig = settings.try_deserialize()?;

    // Apply CLI overrides.
    if cli.live {
        app_config.execution.live_trading = true;
    }
    if let Some(level) = cli.log_level {
        app_config.telemetry.log_level = level;
    }

    // Initialize telemetry.
    pmhft_telemetry::init_logging(
        &app_config.telemetry.log_level,
        app_config.telemetry.enable_json_logs,
    );

    if let Err(e) = pmhft_telemetry::init_metrics(app_config.telemetry.prometheus_port) {
        tracing::warn!(error = %e, "Failed to start Prometheus exporter (non-fatal)");
    }

    tracing::info!("PMHFT starting");
    tracing::info!(
        live_trading = app_config.execution.live_trading,
        dome_tier = app_config.dome.rate_limit_per_sec,
        fix_enabled = app_config.execution.use_fix_for_kalshi,
        "Configuration loaded"
    );

    // Run the application.
    let app = App::new(app_config);
    app.run().await
}
