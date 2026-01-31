use super::codec::{self, tags, FixMessage};
use super::connection::FixTcpConnection;
use super::messages::{self, msg_types, ExecutionReport};
use crate::auth::KalshiAuth;
use pmhft_common::{PmhftError, Result};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio::time::{self, Duration};
use tracing::{debug, error, info, warn};

const HEARTBEAT_INTERVAL_SECS: u32 = 30;

/// FIX 4.4 session manager for Kalshi.
///
/// Handles:
/// - Logon with RSA-PSS authentication
/// - Sequence number management
/// - Heartbeat / TestRequest exchange
/// - Message sending and receiving
/// - Auto-reconnect on disconnect
pub struct FixSession {
    sender_comp_id: String,
    target_comp_id: String,
    auth: Arc<KalshiAuth>,
    outgoing_seq: AtomicU32,
    incoming_seq: AtomicU32,
    conn: Arc<Mutex<Option<FixTcpConnection>>>,
    host: String,
    port: u16,
    exec_report_tx: broadcast::Sender<ExecutionReport>,
    send_tx: mpsc::Sender<Vec<u8>>,
    send_rx: Arc<Mutex<mpsc::Receiver<Vec<u8>>>>,
}

impl FixSession {
    pub fn new(
        host: &str,
        port: u16,
        sender_comp_id: &str,
        target_comp_id: &str,
        auth: KalshiAuth,
    ) -> Self {
        let (exec_report_tx, _) = broadcast::channel(1024);
        let (send_tx, send_rx) = mpsc::channel(256);

        Self {
            sender_comp_id: sender_comp_id.to_string(),
            target_comp_id: target_comp_id.to_string(),
            auth: Arc::new(auth),
            outgoing_seq: AtomicU32::new(1),
            incoming_seq: AtomicU32::new(1),
            conn: Arc::new(Mutex::new(None)),
            host: host.to_string(),
            port,
            exec_report_tx,
            send_tx,
            send_rx: Arc::new(Mutex::new(send_rx)),
        }
    }

    /// Subscribe to execution reports.
    pub fn subscribe_exec_reports(&self) -> broadcast::Receiver<ExecutionReport> {
        self.exec_report_tx.subscribe()
    }

    /// Get the next outgoing sequence number.
    fn next_seq(&self) -> u32 {
        self.outgoing_seq.fetch_add(1, Ordering::SeqCst)
    }

    /// Get the current FIX sending time string.
    fn sending_time() -> String {
        chrono::Utc::now()
            .format("%Y%m%d-%H:%M:%S%.3f")
            .to_string()
    }

    /// Encode and queue a FIX message for sending.
    pub async fn send_message(&self, msg: FixMessage) -> Result<()> {
        let seq = self.next_seq();
        let encoded = msg.encode(
            &self.sender_comp_id,
            &self.target_comp_id,
            seq,
            &Self::sending_time(),
        );
        self.send_tx
            .send(encoded)
            .await
            .map_err(|e| PmhftError::KalshiFix(format!("Send channel closed: {}", e)))?;
        Ok(())
    }

    /// Send a NewOrderSingle and return immediately.
    /// The ExecutionReport will arrive asynchronously via the broadcast channel.
    pub async fn send_new_order(
        &self,
        cl_ord_id: &str,
        ticker: &str,
        fix_side: &str,
        quantity: u32,
        price_cents: u32,
        tif: &str,
    ) -> Result<()> {
        let msg =
            messages::build_new_order_single(cl_ord_id, ticker, fix_side, quantity, price_cents, tif);
        self.send_message(msg).await
    }

    /// Send an OrderCancelRequest.
    pub async fn send_cancel(
        &self,
        cl_ord_id: &str,
        orig_cl_ord_id: &str,
        ticker: &str,
        fix_side: &str,
        quantity: u32,
    ) -> Result<()> {
        let msg = messages::build_order_cancel_request(
            cl_ord_id,
            orig_cl_ord_id,
            ticker,
            fix_side,
            quantity,
        );
        self.send_message(msg).await
    }

    /// Connect to the FIX gateway and run the session loop.
    /// This blocks until the session is disconnected.
    pub async fn run(&self) -> Result<()> {
        loop {
            match self.run_session().await {
                Ok(()) => {
                    info!("FIX session ended gracefully");
                    return Ok(());
                }
                Err(e) => {
                    error!(error = %e, "FIX session error, reconnecting...");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    // Reset sequence numbers on reconnect.
                    self.outgoing_seq.store(1, Ordering::SeqCst);
                    self.incoming_seq.store(1, Ordering::SeqCst);
                }
            }
        }
    }

    async fn run_session(&self) -> Result<()> {
        // Connect TCP + TLS.
        let tcp_conn = FixTcpConnection::connect(&self.host, self.port)
            .await
            .map_err(|e| PmhftError::KalshiFix(format!("Connection failed: {}", e)))?;

        {
            let mut conn_guard = self.conn.lock().await;
            *conn_guard = Some(tcp_conn);
        }

        // Send Logon.
        let (timestamp, signature) = self
            .auth
            .sign_fix_logon(&self.sender_comp_id, &self.target_comp_id);
        // Combine timestamp and signature as the password field.
        let password = format!("{}|{}", timestamp, signature);
        let logon_msg = messages::build_logon(HEARTBEAT_INTERVAL_SECS, true, &password);
        self.send_message(logon_msg).await?;
        info!("FIX Logon sent");

        // Main session loop: read messages, process heartbeats, send queued messages.
        let mut heartbeat_timer =
            time::interval(Duration::from_secs(HEARTBEAT_INTERVAL_SECS as u64));
        heartbeat_timer.tick().await; // skip first tick

        let mut send_rx = self.send_rx.lock().await;
        let mut remainder: Vec<u8> = Vec::new();

        loop {
            tokio::select! {
                // Process outgoing messages.
                Some(data) = send_rx.recv() => {
                    let mut conn_guard = self.conn.lock().await;
                    if let Some(conn) = conn_guard.as_mut() {
                        conn.send(&data).await
                            .map_err(|e| PmhftError::KalshiFix(format!("Send error: {}", e)))?;
                    }
                }

                // Read incoming messages.
                _ = async {
                    let mut conn_guard = self.conn.lock().await;
                    if let Some(conn) = conn_guard.as_mut() {
                        conn.put_back(std::mem::take(&mut remainder));
                        match conn.read().await {
                            Ok(0) => {
                                // Connection closed.
                            }
                            Ok(_n) => {
                                let buf = conn.take_buffer();
                                let (messages, rem) = codec::split_messages(&buf);
                                remainder = rem;

                                for raw_msg in messages {
                                    if let Some(fix_msg) = FixMessage::parse(&raw_msg) {
                                        self.handle_incoming(&fix_msg);
                                    }
                                }
                            }
                            Err(e) => {
                                error!(error = %e, "FIX read error");
                            }
                        }
                    }
                } => {}

                // Send heartbeat.
                _ = heartbeat_timer.tick() => {
                    let hb = messages::build_heartbeat(None);
                    // Ignore errors during heartbeat — session may be closing.
                    let _ = self.send_message(hb).await;
                }
            }
        }
    }

    fn handle_incoming(&self, msg: &FixMessage) {
        let expected_seq = self.incoming_seq.load(Ordering::SeqCst);
        if let Some(seq_str) = msg.get(tags::MSG_SEQ_NUM) {
            if let Ok(seq) = seq_str.parse::<u32>() {
                if seq > expected_seq {
                    warn!(
                        expected = expected_seq,
                        received = seq,
                        "FIX sequence gap detected"
                    );
                }
                self.incoming_seq.store(seq + 1, Ordering::SeqCst);
            }
        }

        match msg.msg_type() {
            msg_types::LOGON => {
                info!("FIX Logon confirmed");
            }
            msg_types::LOGOUT => {
                info!(
                    text = msg.get(tags::TEXT).unwrap_or(""),
                    "FIX Logout received"
                );
            }
            msg_types::HEARTBEAT => {
                debug!("FIX Heartbeat received");
            }
            msg_types::TEST_REQUEST => {
                if let Some(test_req_id) = msg.get(tags::TEST_REQ_ID) {
                    debug!(test_req_id = test_req_id, "FIX TestRequest received");
                    let hb = messages::build_heartbeat(Some(test_req_id));
                    let tx = self.send_tx.clone();
                    let sender = self.sender_comp_id.clone();
                    let target = self.target_comp_id.clone();
                    let seq = self.next_seq();
                    tokio::spawn(async move {
                        let data = hb.encode(&sender, &target, seq, &Self::sending_time());
                        let _ = tx.send(data).await;
                    });
                }
            }
            msg_types::EXECUTION_REPORT => {
                if let Some(report) = ExecutionReport::from_fix(msg) {
                    debug!(
                        cl_ord_id = %report.cl_ord_id,
                        exec_type = %report.exec_type,
                        ord_status = %report.ord_status,
                        "ExecutionReport received"
                    );
                    let _ = self.exec_report_tx.send(report);
                }
            }
            msg_types::REJECT => {
                warn!(
                    text = msg.get(tags::TEXT).unwrap_or(""),
                    "FIX Reject received"
                );
            }
            msg_types::ORDER_CANCEL_REJECT => {
                warn!(
                    text = msg.get(tags::TEXT).unwrap_or(""),
                    cl_ord_id = msg.get(tags::CL_ORD_ID).unwrap_or(""),
                    "FIX OrderCancelReject received"
                );
            }
            other => {
                debug!(msg_type = other, "Unhandled FIX message type");
            }
        }
    }
}
