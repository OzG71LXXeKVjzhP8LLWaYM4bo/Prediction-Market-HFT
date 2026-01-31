use pmhft_common::NormalizedQuote;
use pmhft_dome_client::types::DomeWsMessage;
use pmhft_dome_client::ws::handler::parse_order_event;
use tokio::sync::broadcast;
use tracing::warn;

/// Processes Dome WebSocket events into NormalizedQuote updates for Polymarket.
pub struct PolymarketFeed {
    ws_rx: broadcast::Receiver<DomeWsMessage>,
    quote_tx: broadcast::Sender<NormalizedQuote>,
}

impl PolymarketFeed {
    pub fn new(
        ws_rx: broadcast::Receiver<DomeWsMessage>,
        quote_tx: broadcast::Sender<NormalizedQuote>,
    ) -> Self {
        Self { ws_rx, quote_tx }
    }

    /// Run the feed processing loop.
    pub async fn run(&mut self) {
        loop {
            match self.ws_rx.recv().await {
                Ok(msg) => {
                    if let Some(event) = msg.data.as_ref() {
                        if let Some(quote) = parse_order_event(event) {
                            let _ = self.quote_tx.send(quote);
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(skipped = n, "Polymarket feed lagged, dropped messages");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    warn!("Polymarket feed channel closed");
                    break;
                }
            }
        }
    }
}
