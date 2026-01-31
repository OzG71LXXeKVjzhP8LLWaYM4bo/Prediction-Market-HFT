use crate::types::{DomeWsMessage, DomeWsSubscription};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::broadcast;
use tokio::time::{self, Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{error, info, warn};

const RECONNECT_BASE_DELAY_MS: u64 = 1000;
const RECONNECT_MAX_DELAY_MS: u64 = 30000;
const PING_INTERVAL_SECS: u64 = 30;

/// Dome WebSocket connection manager.
///
/// Connects to `wss://ws.domeapi.io/<API_KEY>`, subscribes by condition_ids,
/// and broadcasts parsed order events.
pub struct DomeWsConnection {
    ws_url: String,
    api_key: String,
    event_tx: broadcast::Sender<DomeWsMessage>,
    condition_ids: Vec<String>,
}

impl DomeWsConnection {
    pub fn new(ws_url: &str, api_key: &str) -> (Self, broadcast::Receiver<DomeWsMessage>) {
        let (event_tx, event_rx) = broadcast::channel(4096);
        (
            Self {
                ws_url: ws_url.to_string(),
                api_key: api_key.to_string(),
                event_tx,
                condition_ids: Vec::new(),
            },
            event_rx,
        )
    }

    pub fn subscribe_receiver(&self) -> broadcast::Receiver<DomeWsMessage> {
        self.event_tx.subscribe()
    }

    /// Set the condition_ids to subscribe to.
    pub fn set_subscriptions(&mut self, condition_ids: Vec<String>) {
        self.condition_ids = condition_ids;
    }

    /// Run the WebSocket connection loop with auto-reconnect.
    pub async fn run(&self) {
        let mut attempt = 0u32;

        loop {
            let url = format!("{}/{}", self.ws_url, self.api_key);
            info!(url = %self.ws_url, "Connecting to Dome WebSocket");

            match connect_async(&url).await {
                Ok((ws_stream, _response)) => {
                    attempt = 0;
                    info!("Dome WebSocket connected");

                    let (mut write, mut read) = ws_stream.split();

                    // Send subscription message.
                    if !self.condition_ids.is_empty() {
                        let sub = DomeWsSubscription {
                            msg_type: "subscribe".to_string(),
                            condition_ids: self.condition_ids.clone(),
                        };
                        if let Ok(json) = serde_json::to_string(&sub) {
                            if let Err(e) = write.send(Message::Text(json.into())).await {
                                error!(error = %e, "Failed to send subscription");
                                continue;
                            }
                            info!(
                                count = self.condition_ids.len(),
                                "Subscribed to condition_ids"
                            );
                        }
                    }

                    // Read loop with periodic ping.
                    let mut ping_interval = time::interval(Duration::from_secs(PING_INTERVAL_SECS));
                    ping_interval.tick().await; // skip first immediate tick

                    loop {
                        tokio::select! {
                            msg = read.next() => {
                                match msg {
                                    Some(Ok(Message::Text(text))) => {
                                        match serde_json::from_str::<DomeWsMessage>(&text) {
                                            Ok(event) => {
                                                let _ = self.event_tx.send(event);
                                            }
                                            Err(e) => {
                                                warn!(error = %e, raw = %text, "Failed to parse WS message");
                                            }
                                        }
                                    }
                                    Some(Ok(Message::Ping(data))) => {
                                        let _ = write.send(Message::Pong(data)).await;
                                    }
                                    Some(Ok(Message::Close(_))) => {
                                        info!("Dome WebSocket closed by server");
                                        break;
                                    }
                                    Some(Err(e)) => {
                                        error!(error = %e, "Dome WebSocket error");
                                        break;
                                    }
                                    None => {
                                        info!("Dome WebSocket stream ended");
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                            _ = ping_interval.tick() => {
                                if let Err(e) = write.send(Message::Ping(vec![].into())).await {
                                    error!(error = %e, "Failed to send ping");
                                    break;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    error!(error = %e, "Failed to connect to Dome WebSocket");
                }
            }

            // Exponential backoff before reconnect.
            attempt += 1;
            let delay = std::cmp::min(
                RECONNECT_BASE_DELAY_MS * 2u64.pow(attempt.min(10)),
                RECONNECT_MAX_DELAY_MS,
            );
            warn!(delay_ms = delay, attempt = attempt, "Reconnecting to Dome WebSocket");
            time::sleep(Duration::from_millis(delay)).await;
        }
    }
}
