use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// HMAC-SHA256 credentials for Polymarket's L2 CLOB API.
///
/// These are separate from the EIP-712 signing key and are used
/// for HTTP API authentication (the "L2 API key" flow).
pub struct ClobCredentials {
    pub api_key: String,
    pub secret: String,
    pub passphrase: String,
}

impl ClobCredentials {
    pub fn new(api_key: &str, secret: &str, passphrase: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            secret: secret.to_string(),
            passphrase: passphrase.to_string(),
        }
    }

    /// Generate HMAC-SHA256 signature for a CLOB API request.
    ///
    /// message = timestamp + METHOD + path + body
    pub fn sign(&self, timestamp: &str, method: &str, path: &str, body: &str) -> String {
        let message = format!("{}{}{}{}", timestamp, method.to_uppercase(), path, body);
        let secret_bytes = STANDARD
            .decode(&self.secret)
            .expect("invalid base64 secret");

        let mut mac =
            HmacSha256::new_from_slice(&secret_bytes).expect("HMAC can take key of any size");
        mac.update(message.as_bytes());
        let result = mac.finalize();

        STANDARD.encode(result.into_bytes())
    }

    /// Apply L2 auth headers to a reqwest RequestBuilder.
    pub fn apply_headers(
        &self,
        builder: reqwest::RequestBuilder,
        method: &str,
        path: &str,
        body: &str,
    ) -> reqwest::RequestBuilder {
        let timestamp = chrono::Utc::now().timestamp().to_string();
        let signature = self.sign(&timestamp, method, path, body);

        builder
            .header("POLY_API_KEY", &self.api_key)
            .header("POLY_SIGNATURE", &signature)
            .header("POLY_TIMESTAMP", &timestamp)
            .header("POLY_PASSPHRASE", &self.passphrase)
    }
}
