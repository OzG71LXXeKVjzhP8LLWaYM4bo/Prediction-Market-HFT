use super::codec::{tags, FixMessage};

/// FIX message type constants.
pub mod msg_types {
    pub const LOGON: &str = "A";
    pub const LOGOUT: &str = "5";
    pub const HEARTBEAT: &str = "0";
    pub const TEST_REQUEST: &str = "1";
    pub const RESEND_REQUEST: &str = "2";
    pub const REJECT: &str = "3";
    pub const SEQUENCE_RESET: &str = "4";
    pub const NEW_ORDER_SINGLE: &str = "D";
    pub const ORDER_CANCEL_REQUEST: &str = "F";
    pub const EXECUTION_REPORT: &str = "8";
    pub const ORDER_CANCEL_REJECT: &str = "9";
}

/// Build a Logon (35=A) message with RSA-PSS authentication.
pub fn build_logon(
    heartbeat_interval: u32,
    reset_seq_num: bool,
    password_signature: &str,
) -> FixMessage {
    let mut msg = FixMessage::new(msg_types::LOGON);
    msg.set(tags::ENCRYPT_METHOD, 0);
    msg.set(tags::HEARTBEAT_INT, heartbeat_interval);
    if reset_seq_num {
        msg.set(tags::RESET_SEQ_NUM, "Y");
    }
    // Kalshi uses tag 554 (Password) for the RSA-PSS signature.
    msg.set(tags::PASSWORD, password_signature);
    msg
}

/// Build a Logout (35=5) message.
pub fn build_logout(text: Option<&str>) -> FixMessage {
    let mut msg = FixMessage::new(msg_types::LOGOUT);
    if let Some(t) = text {
        msg.set(tags::TEXT, t);
    }
    msg
}

/// Build a Heartbeat (35=0) message, optionally in response to a TestRequest.
pub fn build_heartbeat(test_req_id: Option<&str>) -> FixMessage {
    let mut msg = FixMessage::new(msg_types::HEARTBEAT);
    if let Some(id) = test_req_id {
        msg.set(tags::TEST_REQ_ID, id);
    }
    msg
}

/// Build a TestRequest (35=1) message.
pub fn build_test_request(test_req_id: &str) -> FixMessage {
    let mut msg = FixMessage::new(msg_types::TEST_REQUEST);
    msg.set(tags::TEST_REQ_ID, test_req_id);
    msg
}

/// FIX Side values.
pub mod side {
    pub const BUY: &str = "1";
    pub const SELL: &str = "2";
}

/// FIX OrdType values.
pub mod ord_type {
    pub const LIMIT: &str = "2";
}

/// FIX TimeInForce values.
pub mod time_in_force {
    pub const DAY: &str = "0";
    pub const GTC: &str = "1";
    pub const IOC: &str = "3"; // Immediate or Cancel (FAK)
    pub const FOK: &str = "4"; // Fill or Kill
}

/// Build a NewOrderSingle (35=D) message.
pub fn build_new_order_single(
    cl_ord_id: &str,
    ticker: &str,
    fix_side: &str,
    quantity: u32,
    price_cents: u32,
    tif: &str,
) -> FixMessage {
    let mut msg = FixMessage::new(msg_types::NEW_ORDER_SINGLE);
    msg.set(tags::CL_ORD_ID, cl_ord_id);
    msg.set(tags::SYMBOL, ticker);
    msg.set(tags::SIDE, fix_side);
    msg.set(tags::ORDER_QTY, quantity);
    msg.set(tags::ORD_TYPE, ord_type::LIMIT);
    msg.set(tags::PRICE, price_cents);
    msg.set(tags::TIME_IN_FORCE, tif);
    msg.set(tags::TRANSACT_TIME, chrono::Utc::now().format("%Y%m%d-%H:%M:%S%.3f").to_string());
    msg
}

/// Build an OrderCancelRequest (35=F) message.
pub fn build_order_cancel_request(
    cl_ord_id: &str,
    orig_cl_ord_id: &str,
    ticker: &str,
    fix_side: &str,
    quantity: u32,
) -> FixMessage {
    let mut msg = FixMessage::new(msg_types::ORDER_CANCEL_REQUEST);
    msg.set(tags::CL_ORD_ID, cl_ord_id);
    msg.set(tags::ORIG_CL_ORD_ID, orig_cl_ord_id);
    msg.set(tags::SYMBOL, ticker);
    msg.set(tags::SIDE, fix_side);
    msg.set(tags::ORDER_QTY, quantity);
    msg.set(tags::TRANSACT_TIME, chrono::Utc::now().format("%Y%m%d-%H:%M:%S%.3f").to_string());
    msg
}

/// Parsed execution report fields.
#[derive(Debug, Clone)]
pub struct ExecutionReport {
    pub order_id: String,
    pub cl_ord_id: String,
    pub exec_id: String,
    pub exec_type: String,
    pub ord_status: String,
    pub symbol: String,
    pub side: String,
    pub last_qty: Option<u32>,
    pub last_px: Option<u32>,
    pub leaves_qty: u32,
    pub cum_qty: u32,
    pub avg_px: Option<f64>,
    pub text: Option<String>,
}

impl ExecutionReport {
    /// Parse an ExecutionReport from a FIX message.
    pub fn from_fix(msg: &FixMessage) -> Option<Self> {
        Some(Self {
            order_id: msg.get(tags::ORDER_ID)?.to_string(),
            cl_ord_id: msg.get(tags::CL_ORD_ID)?.to_string(),
            exec_id: msg.get(tags::EXEC_ID)?.to_string(),
            exec_type: msg.get(tags::EXEC_TYPE)?.to_string(),
            ord_status: msg.get(tags::ORD_STATUS)?.to_string(),
            symbol: msg.get(tags::SYMBOL).unwrap_or("").to_string(),
            side: msg.get(tags::SIDE).unwrap_or("").to_string(),
            last_qty: msg.get(tags::LAST_QTY).and_then(|v| v.parse().ok()),
            last_px: msg.get(tags::LAST_PX).and_then(|v| v.parse().ok()),
            leaves_qty: msg
                .get(tags::LEAVES_QTY)
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            cum_qty: msg
                .get(tags::CUM_QTY)
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            avg_px: msg.get(tags::AVG_PX).and_then(|v| v.parse().ok()),
            text: msg.get(tags::TEXT).map(|s| s.to_string()),
        })
    }
}
