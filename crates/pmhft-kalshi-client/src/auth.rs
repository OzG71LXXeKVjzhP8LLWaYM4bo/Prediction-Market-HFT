use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use pmhft_common::{PmhftError, Result};
use rsa::pkcs8::DecodePrivateKey;
use rsa::pss::BlindedSigningKey;
use rsa::sha2::Sha256;
use rsa::signature::{RandomizedSigner, SignatureEncoding};
use rsa::RsaPrivateKey;
use std::time::{SystemTime, UNIX_EPOCH};

/// RSA-PSS SHA-256 authentication for Kalshi API.
///
/// Signing scheme:
///   message = timestamp_ms + METHOD + path (no query params)
///   signature = RSA-PSS-SHA256(private_key, message) -> base64
pub struct KalshiAuth {
    key_id: String,
    signing_key: BlindedSigningKey<Sha256>,
}

impl KalshiAuth {
    /// Create from a PEM-encoded PKCS#8 RSA private key file.
    pub fn from_pem_file(key_id: String, pem_path: &str) -> Result<Self> {
        let pem_contents = std::fs::read_to_string(pem_path).map_err(|e| {
            PmhftError::RsaSigning(format!("Failed to read PEM file '{}': {}", pem_path, e))
        })?;
        Self::from_pem(key_id, &pem_contents)
    }

    /// Create from a PEM string.
    pub fn from_pem(key_id: String, pem: &str) -> Result<Self> {
        let private_key = RsaPrivateKey::from_pkcs8_pem(pem)
            .map_err(|e| PmhftError::RsaSigning(format!("Failed to parse PEM: {}", e)))?;
        let signing_key = BlindedSigningKey::<Sha256>::new(private_key);
        Ok(Self {
            key_id,
            signing_key,
        })
    }

    /// Sign a request, returning (timestamp_ms_string, base64_signature).
    pub fn sign_request(&self, method: &str, path: &str) -> (String, String) {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let timestamp_str = timestamp_ms.to_string();

        // Strip query parameters from path.
        let path_clean = path.split('?').next().unwrap_or(path);

        // Kalshi message format: timestamp_ms + METHOD + path
        let message = format!("{}{}{}", timestamp_str, method.to_uppercase(), path_clean);

        let mut rng = rand::thread_rng();
        let signature = self.signing_key.sign_with_rng(&mut rng, message.as_bytes());
        let sig_b64 = STANDARD.encode(signature.to_bytes());

        (timestamp_str, sig_b64)
    }

    /// Apply auth headers to a reqwest::RequestBuilder.
    pub fn apply_headers(
        &self,
        builder: reqwest::RequestBuilder,
        method: &str,
        path: &str,
    ) -> reqwest::RequestBuilder {
        let (ts, sig) = self.sign_request(method, path);
        builder
            .header("KALSHI-ACCESS-KEY", &self.key_id)
            .header("KALSHI-ACCESS-TIMESTAMP", &ts)
            .header("KALSHI-ACCESS-SIGNATURE", &sig)
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    /// Sign a FIX logon message. Returns (timestamp_ms_string, base64_signature).
    pub fn sign_fix_logon(&self, sender_comp_id: &str, target_comp_id: &str) -> (String, String) {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let timestamp_str = timestamp_ms.to_string();

        // FIX logon signing: timestamp + SenderCompID + TargetCompID
        let message = format!("{}{}{}", timestamp_str, sender_comp_id, target_comp_id);

        let mut rng = rand::thread_rng();
        let signature = self.signing_key.sign_with_rng(&mut rng, message.as_bytes());
        let sig_b64 = STANDARD.encode(signature.to_bytes());

        (timestamp_str, sig_b64)
    }
}
