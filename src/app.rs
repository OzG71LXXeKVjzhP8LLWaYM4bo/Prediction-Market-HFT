use pmhft_common::config::AppConfig;
use pmhft_common::NormalizedQuote;
use pmhft_dome_client::rest::{DomeKalshiClient, DomePolymarketClient};
use pmhft_dome_client::ws::DomeWsConnection;
use pmhft_execution::{ExecutionEngine, KalshiExecutor, PolymarketExecutor};
use pmhft_market_data::{KalshiFeed, MarketDataAggregator, PolymarketFeed};
use pmhft_matching::{PairReconciler, PairRegistry};
use pmhft_risk::{CircuitBreaker, PositionTracker, RiskManager};
use pmhft_strategy::ArbDetector;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tokio::time::Duration;
use tracing::{error, info};

/// Application orchestrator.
/// Wires all subsystems together and manages their lifecycles.
pub struct App {
    config: AppConfig,
}

impl App {
    pub fn new(config: AppConfig) -> Self {
        Self { config }
    }

    pub async fn run(self) -> anyhow::Result<()> {
        info!("Initializing PMHFT subsystems");

        // ── Shared state ──
        let pair_registry = Arc::new(PairRegistry::new());
        let positions = Arc::new(PositionTracker::new());
        let circuit_breaker = Arc::new(CircuitBreaker::new());
        let risk_manager = Arc::new(RiskManager::new(
            self.config.risk.clone(),
            positions.clone(),
            circuit_breaker.clone(),
        ));

        // ── Market data channels ──
        let (quote_tx, quote_rx) = broadcast::channel::<NormalizedQuote>(8192);

        // ── Dome WebSocket (Polymarket real-time data) ──
        let (dome_ws, dome_ws_rx) =
            DomeWsConnection::new(&self.config.dome.ws_url, &self.config.dome.api_key);

        // ── Polymarket feed (WS events -> normalized quotes) ──
        let mut poly_feed = PolymarketFeed::new(dome_ws_rx, quote_tx.clone());

        // ── Kalshi feed (REST polling via Dome -> normalized quotes) ──
        let dome_kalshi_client = DomeKalshiClient::new(
            &self.config.dome.base_url,
            &self.config.dome.api_key,
            self.config.dome.rate_limit_per_sec,
            self.config.dome.request_timeout_ms,
        );
        let kalshi_feed = KalshiFeed::new(
            dome_kalshi_client,
            pair_registry.clone(),
            quote_tx.clone(),
            // Poll interval: spread requests across pairs at 1/s for free tier.
            Duration::from_secs(1),
        );

        // ── Market data aggregator ──
        let (mut aggregator, _unified_rx) = MarketDataAggregator::new(quote_rx);

        // ── Pair reconciler (sports + fuzzy matching) ──
        let reconciler = PairReconciler::new(
            pair_registry.clone(),
            &self.config.dome,
            &self.config.matching,
        );

        // ── Arb detection engine ──
        let (signal_tx, signal_rx) = mpsc::channel(256);
        let _arb_detector = ArbDetector::new(self.config.strategy.clone(), signal_tx);

        // ── Execution engine ──
        let dome_poly_client = Arc::new(DomePolymarketClient::new(
            &self.config.dome.base_url,
            &self.config.dome.api_key,
            self.config.dome.rate_limit_per_sec,
            self.config.dome.request_timeout_ms,
        ));

        let poly_executor =
            PolymarketExecutor::new(dome_poly_client, self.config.execution.clone());

        // Kalshi executor: FIX session is optional (depends on config).
        let kalshi_executor =
            KalshiExecutor::new(None, None, self.config.execution.clone());

        let mut execution_engine = ExecutionEngine::new(
            risk_manager.clone(),
            poly_executor,
            kalshi_executor,
            signal_rx,
        );

        // ── Initial pair discovery ──
        info!("Running initial pair discovery");
        if let Err(e) = reconciler.refresh_once().await {
            error!(error = %e, "Initial pair discovery failed (non-fatal, will retry)");
        }
        info!(pairs = pair_registry.len(), "Initial pairs discovered");

        // ── Set Dome WS subscriptions from discovered pairs ──
        // (Would need mutable access to dome_ws here; in production,
        // subscriptions are managed via the reconciler.)

        // ── Spawn all subsystem tasks ──
        info!(
            live = self.config.execution.live_trading,
            pairs = pair_registry.len(),
            "Starting all subsystems"
        );

        let handles = vec![
            // Dome WebSocket connection.
            tokio::spawn(async move {
                dome_ws.run().await;
                Ok::<(), anyhow::Error>(())
            }),
            // Polymarket feed processor.
            tokio::spawn(async move {
                poly_feed.run().await;
                Ok(())
            }),
            // Kalshi REST polling feed.
            tokio::spawn(async move {
                kalshi_feed.run().await;
                Ok(())
            }),
            // Market data aggregator.
            tokio::spawn(async move {
                aggregator.run().await;
                Ok(())
            }),
            // Pair reconciler.
            tokio::spawn(async move {
                reconciler.run_loop().await;
                Ok(())
            }),
            // Execution engine.
            tokio::spawn(async move {
                execution_engine.run().await;
                Ok(())
            }),
        ];

        // Wait for any task to exit (indicates an error or shutdown).
        let (result, index, _remaining) = futures::future::select_all(handles).await;
        error!(
            task_index = index,
            result = ?result,
            "Subsystem task exited"
        );

        Ok(())
    }
}
