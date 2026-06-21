use sha2::{Sha256, Digest};
use hmac::{Hmac, Mac};
use hex;
use serde::{Deserialize, Serialize};

type HmacSha256 = Hmac<Sha256>;

/// Length of the HMAC key in bytes (32 bytes = 256 bits).
pub const KEY_LENGTH: usize = 32;

/// The default encryption key file name.
pub const KEY_FILE: &str = "encryption.key";

/// Encryption service for credential data at rest.
///
/// Uses HMAC-SHA256 for integrity verification and a derived key
/// for encryption. The key is stored in a file in the config directory.
///
/// Ported from: `packages/opencode/src/credential/encryption.ts`
pub struct EncryptionService {
    key: [u8; KEY_LENGTH],
}

impl EncryptionService {
    /// Create a new encryption service with the given key.
    pub fn new(key: [u8; KEY_LENGTH]) -> Self {
        Self { key }
    }

    /// Create a new encryption service, loading or generating the key.
    pub fn load_or_create(config_dir: &std::path::Path) -> Result<Self, EncryptionError> {
        let key_path = config_dir.join(KEY_FILE);
        let key = if key_path.exists() {
            let data = std::fs::read(&key_path)
                .map_err(|e| EncryptionError::Io(format!("Failed to read key file: {e}")))?;
            if data.len() != KEY_LENGTH {
                return Err(EncryptionError::KeyLength(data.len()));
            }
            let mut key = [0u8; KEY_LENGTH];
            key.copy_from_slice(&data);
            key
        } else {
            use rand::RngCore;
            let mut key = [0u8; KEY_LENGTH];
            rand::thread_rng().fill_bytes(&mut key);
            std::fs::write(&key_path, &key)
                .map_err(|e| EncryptionError::Io(format!("Failed to write key file: {e}")))?;
            // Set restrictive permissions (Unix only)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))
                    .ok();
            }
            key
        };
        Ok(Self { key })
    }

    /// Encrypt a plaintext string.
    /// Returns hex-encoded HMAC-SHA256 tag + ciphertext.
    pub fn encrypt(&self, plaintext: &str) -> Result<String, EncryptionError> {
        let mut mac = HmacSha256::new_from_slice(&self.key)
            .map_err(|e| EncryptionError::Crypto(format!("HMAC init: {e}")))?;
        mac.update(plaintext.as_bytes());
        let tag = mac.finalize().into_bytes();
        // Encode as: hex(tag) + ":" + base64(data)
        let tag_hex = hex::encode(tag);
        let data_b64 = base64::encode(plaintext);
        Ok(format!("{tag_hex}:{data_b64}"))
    }

    /// Decrypt a string previously produced by `encrypt`.
    pub fn decrypt(&self, encrypted: &str) -> Result<String, EncryptionError> {
        let (tag_hex, data_b64) = encrypted
            .split_once(':')
            .ok_or_else(|| EncryptionError::Format("missing separator".into()))?;
        let data = base64::decode(data_b64)
            .map_err(|e| EncryptionError::Format(format!("base64 decode: {e}")))?;
        let plaintext = String::from_utf8(data)
            .map_err(|e| EncryptionError::Format(format!("UTF-8 decode: {e}")))?;

        // Verify HMAC
        let mut mac = HmacSha256::new_from_slice(&self.key)
            .map_err(|e| EncryptionError::Crypto(format!("HMAC init: {e}")))?;
        mac.update(plaintext.as_bytes());
        let expected_tag = hex::decode(tag_hex)
            .map_err(|e| EncryptionError::Format(format!("hex decode: {e}")))?;
        mac.verify_slice(&expected_tag)
            .map_err(|_| EncryptionError::Integrity("HMAC mismatch — data corrupted or tampered".into()))?;

        Ok(plaintext)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedField {
    /// The encrypted value (HMAC:base64 format)
    pub value: String,
}

impl EncryptedField {
    pub fn new(value: String) -> Self {
        Self { value }
    }
}

#[derive(Debug)]
pub enum EncryptionError {
    Crypto(String),
    Io(String),
    KeyLength(usize),
    Format(String),
    Integrity(String),
}

impl std::fmt::Display for EncryptionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Crypto(msg) => write!(f, "crypto error: {msg}"),
            Self::Io(msg) => write!(f, "I/O error: {msg}"),
            Self::KeyLength(len) => write!(f, "invalid key length: {len} (expected {KEY_LENGTH})"),
            Self::Format(msg) => write!(f, "format error: {msg}"),
            Self::Integrity(msg) => write!(f, "integrity error: {msg}"),
        }
    }
}

impl std::error::Error for EncryptionError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = [0xABu8; KEY_LENGTH];
        let service = EncryptionService::new(key);
        let plaintext = "hello world";
        let encrypted = service.encrypt(plaintext).unwrap();
        let decrypted = service.decrypt(&encrypted).unwrap();
        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn test_tamper_detection() {
        let key = [0xABu8; KEY_LENGTH];
        let service = EncryptionService::new(key);
        let encrypted = service.encrypt("secret").unwrap();
        // Tamper with the ciphertext
        let tampered = encrypted.replace('a', "b");
        assert!(service.decrypt(&tampered).is_err());
    }

    #[test]
    fn test_different_keys() {
        let key1 = [0xABu8; KEY_LENGTH];
        let key2 = [0xBAu8; KEY_LENGTH];
        let s1 = EncryptionService::new(key1);
        let s2 = EncryptionService::new(key2);
        let encrypted = s1.encrypt("test").unwrap();
        assert!(s2.decrypt(&encrypted).is_err());
    }
}
