use std::collections::BTreeMap;

/// FIX 4.4 message delimiter (SOH = 0x01).
pub const SOH: char = '\x01';

/// FIX 4.4 begin string.
pub const BEGIN_STRING: &str = "FIX.4.4";

/// Standard FIX tag numbers used by Kalshi.
pub mod tags {
    pub const BEGIN_STRING: u32 = 8;
    pub const BODY_LENGTH: u32 = 9;
    pub const MSG_TYPE: u32 = 35;
    pub const SENDER_COMP_ID: u32 = 49;
    pub const TARGET_COMP_ID: u32 = 56;
    pub const MSG_SEQ_NUM: u32 = 34;
    pub const SENDING_TIME: u32 = 52;
    pub const CHECKSUM: u32 = 10;
    pub const ENCRYPT_METHOD: u32 = 98;
    pub const HEARTBEAT_INT: u32 = 108;
    pub const RESET_SEQ_NUM: u32 = 141;
    pub const PASSWORD: u32 = 554;
    pub const TEXT: u32 = 58;
    pub const TEST_REQ_ID: u32 = 112;

    // Application-level tags
    pub const CL_ORD_ID: u32 = 11;
    pub const ORDER_ID: u32 = 37;
    pub const EXEC_ID: u32 = 17;
    pub const EXEC_TYPE: u32 = 150;
    pub const ORD_STATUS: u32 = 39;
    pub const SYMBOL: u32 = 55;
    pub const SIDE: u32 = 54;
    pub const ORDER_QTY: u32 = 38;
    pub const ORD_TYPE: u32 = 40;
    pub const PRICE: u32 = 44;
    pub const TIME_IN_FORCE: u32 = 59;
    pub const TRANSACT_TIME: u32 = 60;
    pub const LAST_QTY: u32 = 32;
    pub const LAST_PX: u32 = 31;
    pub const LEAVES_QTY: u32 = 151;
    pub const CUM_QTY: u32 = 14;
    pub const AVG_PX: u32 = 6;
    pub const ORIG_CL_ORD_ID: u32 = 41;

    // Kalshi-specific custom tags
    pub const KALSHI_ACTION: u32 = 20000;
    pub const KALSHI_TICKER: u32 = 55; // reuse Symbol
}

/// A parsed FIX message as an ordered map of tag -> value.
#[derive(Debug, Clone)]
pub struct FixMessage {
    fields: BTreeMap<u32, String>,
    msg_type: String,
}

impl FixMessage {
    pub fn new(msg_type: &str) -> Self {
        Self {
            fields: BTreeMap::new(),
            msg_type: msg_type.to_string(),
        }
    }

    pub fn set(&mut self, tag: u32, value: impl ToString) {
        self.fields.insert(tag, value.to_string());
    }

    pub fn get(&self, tag: u32) -> Option<&str> {
        self.fields.get(&tag).map(|s| s.as_str())
    }

    pub fn msg_type(&self) -> &str {
        &self.msg_type
    }

    /// Encode this message into a FIX 4.4 wire format string.
    ///
    /// Structure: 8=FIX.4.4|9=<body_length>|<body>|10=<checksum>|
    /// where body = 35=<type>| + all other fields (excluding 8, 9, 10).
    pub fn encode(
        &self,
        sender_comp_id: &str,
        target_comp_id: &str,
        seq_num: u32,
        sending_time: &str,
    ) -> Vec<u8> {
        // Build body (everything between 9= and 10=).
        let mut body = String::new();

        // MsgType first.
        body.push_str(&format!("35={}{}", self.msg_type, SOH));
        body.push_str(&format!("49={}{}", sender_comp_id, SOH));
        body.push_str(&format!("56={}{}", target_comp_id, SOH));
        body.push_str(&format!("34={}{}", seq_num, SOH));
        body.push_str(&format!("52={}{}", sending_time, SOH));

        // All other fields (skip tags we already included).
        for (&tag, value) in &self.fields {
            if tag == tags::BEGIN_STRING
                || tag == tags::BODY_LENGTH
                || tag == tags::MSG_TYPE
                || tag == tags::SENDER_COMP_ID
                || tag == tags::TARGET_COMP_ID
                || tag == tags::MSG_SEQ_NUM
                || tag == tags::SENDING_TIME
                || tag == tags::CHECKSUM
            {
                continue;
            }
            body.push_str(&format!("{}={}{}", tag, value, SOH));
        }

        // Compute body length (bytes of body).
        let body_len = body.len();

        // Build header.
        let header = format!("8={}{}", BEGIN_STRING, SOH);
        let length_field = format!("9={}{}", body_len, SOH);

        // Full message without checksum.
        let mut full = String::new();
        full.push_str(&header);
        full.push_str(&length_field);
        full.push_str(&body);

        // Compute checksum (sum of all bytes mod 256, zero-padded to 3 digits).
        let checksum: u32 = full.bytes().map(|b| b as u32).sum::<u32>() % 256;
        full.push_str(&format!("10={:03}{}", checksum, SOH));

        full.into_bytes()
    }

    /// Parse a FIX message from raw bytes.
    pub fn parse(data: &[u8]) -> Option<Self> {
        let text = std::str::from_utf8(data).ok()?;
        let mut fields = BTreeMap::new();
        let mut msg_type = String::new();

        for pair in text.split(SOH) {
            if pair.is_empty() {
                continue;
            }
            let mut parts = pair.splitn(2, '=');
            let tag: u32 = parts.next()?.parse().ok()?;
            let value = parts.next()?.to_string();

            if tag == tags::MSG_TYPE {
                msg_type = value.clone();
            }
            fields.insert(tag, value);
        }

        if msg_type.is_empty() {
            return None;
        }

        Some(Self { fields, msg_type })
    }
}

/// Split a byte buffer into complete FIX messages.
/// Returns (messages, remaining_bytes).
pub fn split_messages(buf: &[u8]) -> (Vec<Vec<u8>>, Vec<u8>) {
    let mut messages = Vec::new();
    let mut start = 0;

    while start < buf.len() {
        // Look for the checksum field "10=XXX\x01" which terminates a message.
        if let Some(pos) = find_checksum_end(&buf[start..]) {
            let end = start + pos;
            messages.push(buf[start..end].to_vec());
            start = end;
        } else {
            break;
        }
    }

    let remaining = buf[start..].to_vec();
    (messages, remaining)
}

fn find_checksum_end(buf: &[u8]) -> Option<usize> {
    // Find "10=" followed by 3 digits and SOH.
    let text = std::str::from_utf8(buf).ok()?;
    let marker = "10=";

    let mut search_from = 0;
    while let Some(idx) = text[search_from..].find(marker) {
        let abs_idx = search_from + idx;
        // Check if there are at least 4 more chars (3 digits + SOH).
        if abs_idx + marker.len() + 4 <= text.len() {
            let after = &text[abs_idx + marker.len()..];
            if after.len() >= 4 && after[..3].chars().all(|c| c.is_ascii_digit()) && after.as_bytes()[3] == SOH as u8 {
                return Some(abs_idx + marker.len() + 4);
            }
        }
        search_from = abs_idx + 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_roundtrip() {
        let mut msg = FixMessage::new("A");
        msg.set(tags::ENCRYPT_METHOD, 0);
        msg.set(tags::HEARTBEAT_INT, 30);

        let encoded = msg.encode("SENDER", "TARGET", 1, "20260131-12:00:00.000");
        let parsed = FixMessage::parse(&encoded).unwrap();

        assert_eq!(parsed.msg_type(), "A");
        assert_eq!(parsed.get(tags::SENDER_COMP_ID), Some("SENDER"));
        assert_eq!(parsed.get(tags::TARGET_COMP_ID), Some("TARGET"));
        assert_eq!(parsed.get(tags::MSG_SEQ_NUM), Some("1"));
        assert_eq!(parsed.get(tags::ENCRYPT_METHOD), Some("0"));
        assert_eq!(parsed.get(tags::HEARTBEAT_INT), Some("30"));
    }
}
