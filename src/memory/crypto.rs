//! Encryption at rest for sensitive memory values (S3-T2).
//!
//! AES-256-GCM. A per-user data key is derived from the master key (`MEMORY_MASTER_KEY`) and
//! the user id with HMAC-SHA256, so one leaked row cannot be decrypted without the master key
//! and rotating the master key re-keys every user. Ciphertext is stored as
//! `{"enc":"v1","data":"<base64(nonce || ciphertext)>"}`.

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, Nonce};
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

#[derive(Clone)]
pub struct Crypto {
    master: [u8; 32],
}

impl Crypto {
    /// Any passphrase or base64 key material; it is hashed to 32 bytes.
    pub fn from_master(material: &str) -> Self {
        let mut h = Sha256::new();
        h.update(material.as_bytes());
        let mut master = [0u8; 32];
        master.copy_from_slice(&h.finalize());
        Self { master }
    }

    pub fn from_env_or_dev() -> Self {
        let material = std::env::var("MEMORY_MASTER_KEY")
            .ok()
            .filter(|k| !k.is_empty())
            .unwrap_or_else(|| {
                tracing::warn!("MEMORY_MASTER_KEY unset — using a development key; set it before storing real data");
                "agentrix-dev-memory-master-key".into()
            });
        Self::from_master(&material)
    }

    fn user_key(&self, user_id: &str) -> Key<Aes256Gcm> {
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&self.master).expect("hmac key");
        mac.update(b"memory-data-key:");
        mac.update(user_id.as_bytes());
        let out = mac.finalize().into_bytes();
        *Key::<Aes256Gcm>::from_slice(&out)
    }

    pub fn encrypt(&self, user_id: &str, plaintext: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let cipher = Aes256Gcm::new(&self.user_key(user_id));
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let bytes = serde_json::to_vec(plaintext)?;
        let ct = cipher
            .encrypt(&nonce, bytes.as_ref())
            .map_err(|e| anyhow::anyhow!("encrypt: {e}"))?;
        let mut blob = nonce.to_vec();
        blob.extend_from_slice(&ct);
        Ok(serde_json::json!({ "enc": "v1", "data": B64.encode(blob) }))
    }

    pub fn is_encrypted(value: &serde_json::Value) -> bool {
        value.get("enc").and_then(|v| v.as_str()) == Some("v1") && value.get("data").is_some()
    }

    pub fn decrypt(&self, user_id: &str, stored: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
        if !Self::is_encrypted(stored) {
            return Ok(stored.clone());
        }
        let data = stored["data"].as_str().ok_or_else(|| anyhow::anyhow!("bad ciphertext"))?;
        let blob = B64.decode(data)?;
        anyhow::ensure!(blob.len() > 12, "ciphertext too short");
        let (nonce, ct) = blob.split_at(12);
        let cipher = Aes256Gcm::new(&self.user_key(user_id));
        let pt = cipher
            .decrypt(Nonce::from_slice(nonce), ct)
            .map_err(|e| anyhow::anyhow!("decrypt: {e}"))?;
        Ok(serde_json::from_slice(&pt)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_crypto_roundtrip_and_user_isolation() {
        let c = Crypto::from_master("test-master");
        let v = serde_json::json!({"avg_sleep_h": 6.1, "note": "private"});
        let enc = c.encrypt("user-a", &v).unwrap();
        assert!(Crypto::is_encrypted(&enc));
        assert!(!enc.to_string().contains("private"));
        assert_eq!(c.decrypt("user-a", &enc).unwrap(), v);
        assert!(c.decrypt("user-b", &enc).is_err(), "another user's key must not decrypt");
        assert!(Crypto::from_master("other").decrypt("user-a", &enc).is_err(), "rotated master must not decrypt");
    }
}
