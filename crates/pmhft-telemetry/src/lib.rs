use metrics_exporter_prometheus::PrometheusBuilder;
use tracing_subscriber::{fmt, EnvFilter};

/// Initialize the structured logging system.
pub fn init_logging(log_level: &str, json: bool) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(log_level));

    if json {
        fmt()
            .with_env_filter(filter)
            .json()
            .with_target(true)
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true)
            .init();
    } else {
        fmt()
            .with_env_filter(filter)
            .with_target(true)
            .with_thread_ids(false)
            .init();
    }
}

/// Start the Prometheus metrics exporter on the given port.
/// Returns a handle that keeps the exporter alive.
pub fn init_metrics(port: u16) -> anyhow::Result<()> {
    PrometheusBuilder::new()
        .with_http_listener(([0, 0, 0, 0], port))
        .install()
        .map_err(|e| anyhow::anyhow!("Failed to install Prometheus exporter: {}", e))?;

    tracing::info!(port = port, "Prometheus metrics exporter started");
    Ok(())
}

/// Record a counter increment.
pub fn inc_counter(name: &'static str) {
    metrics::counter!(name).increment(1);
}

/// Record a gauge value.
pub fn set_gauge(name: &'static str, value: f64) {
    metrics::gauge!(name).set(value);
}

/// Record a histogram observation.
pub fn record_histogram(name: &'static str, value: f64) {
    metrics::histogram!(name).record(value);
}
