//! Crypto primitives for the secure store: Argon2id KDF (m=64 MiB, t=3,
//! p=4), AES-256-GCM sealing, and zeroize wrappers.
//!
//! Layout note: `mbxt-storage::crypto` holds the same primitives for the
//! SQLite store with RFC-9106-baseline parameters; this module is the
//! `sessions.enc` path with the prompt-mandated hardening parameters.
//! One of the two should absorb the other once the store format settles
//! (tracked as tech-stack follow-up).

use zeroize::Zeroize;
use zeroize::Zeroizing;

/// AES-256 key length in bytes.
pub const KEY_LEN: usize = 32;
/// Argon2 salt length in bytes.
pub const SALT_LEN: usize = 16;
/// GCM nonce length in bytes (96-bit, random per record).
pub const NONCE_LEN: usize = 12;

/// KDF parameters: Argon2id, m=64 MiB, t=3, p=4.
const ARGON2_M_COST_KIB: u32 = 64 * 1024;
const ARGON2_T_COST: u32 = 3;
const ARGON2_P_COST: u32 = 4;

/// Crypto failures (mapped to friendly UI/CLI messages by callers).
#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("key derivation failed")]
    Kdf,
    #[error("encryption failed")]
    Encrypt,
    #[error("decryption failed (wrong password or corrupted data)")]
    Decrypt,
    #[error("OS entropy unavailable")]
    Entropy,
}

/// Password held in protected memory; zeroized on drop. Use this at every
/// trust boundary (CLI prompt, UI dialog) instead of bare `String`.
#[derive(Clone)]
pub struct SecurePassword {
    inner: Zeroizing<Vec<u8>>,
}

impl std::fmt::Debug for SecurePassword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecurePassword").finish_non_exhaustive()
    }
}

impl SecurePassword {
    /// Copy `password` into zeroizing storage.
    pub fn new(password: &str) -> Self {
        Self {
            inner: Zeroizing::new(password.as_bytes().to_vec()),
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.inner
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.inner).expect("SecurePassword is created from UTF-8")
    }
}

/// Generate a fresh random KDF salt.
pub fn new_salt() -> [u8; SALT_LEN] {
    let mut salt = [0u8; SALT_LEN];
    getrandom::getrandom(&mut salt).expect("OS entropy unavailable");
    salt
}

/// Generate a fresh random GCM nonce (never reuse with the same key!).
pub fn new_nonce() -> [u8; NONCE_LEN] {
    let mut nonce = [0u8; NONCE_LEN];
    getrandom::getrandom(&mut nonce).expect("OS entropy unavailable");
    nonce
}

/// Derive a 32-byte key from `password` and `salt` using Argon2id
/// (m=64 MiB, t=3, p=4).
pub fn derive_key(password: &str, salt: &[u8]) -> Result<[u8; KEY_LEN], CryptoError> {
    use argon2::{Algorithm, Argon2, Params, Version};
    let mut key = [0u8; KEY_LEN];
    let params = Params::new(
        ARGON2_M_COST_KIB,
        ARGON2_T_COST,
        ARGON2_P_COST,
        Some(KEY_LEN),
    )
    .map_err(|_| CryptoError::Kdf)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|_| CryptoError::Kdf)?;
    Ok(key)
}

/// AES-256-GCM seal: returns `ciphertext || tag` (nonce supplied by caller
/// and stored alongside by the caller).
pub fn encrypt(
    key: &[u8; KEY_LEN],
    nonce: &[u8; NONCE_LEN],
    plaintext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    use aes_gcm::aead::{Aead, KeyInit, Payload};
    use aes_gcm::{Aes256Gcm, Nonce};

    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| CryptoError::Kdf)?;
    cipher
        .encrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| CryptoError::Encrypt)
}

/// AES-256-GCM open for data sealed by [`encrypt`].
pub fn decrypt(
    key: &[u8; KEY_LEN],
    nonce: &[u8; NONCE_LEN],
    sealed: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    use aes_gcm::aead::{Aead, KeyInit, Payload};
    use aes_gcm::{Aes256Gcm, Nonce};

    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| CryptoError::Kdf)?;
    cipher
        .decrypt(Nonce::from_slice(nonce), Payload { msg: sealed, aad })
        .map_err(|_| CryptoError::Decrypt)
}

/// Zeroize a plaintext buffer after use (call sites: decrypted session
/// buffers, exported plaintext).
pub fn wipe(buf: &mut [u8]) {
    buf.zeroize();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_key_is_deterministic_and_parameterized() {
        let salt = new_salt();
        let a = derive_key("correct horse battery staple", &salt).unwrap();
        let b = derive_key("correct horse battery staple", &salt).unwrap();
        assert_eq!(a, b);
        let c = derive_key("wrong", &salt).unwrap();
        assert_ne!(a, c);
        // Different salt → different key.
        let d = derive_key("correct horse battery staple", &new_salt()).unwrap();
        assert_ne!(a, d);
    }

    #[test]
    fn encrypt_decrypt_round_trip() {
        let key = derive_key("pw", &new_salt()).unwrap();
        let nonce = new_nonce();
        let sealed = encrypt(&key, &nonce, b"session data", b"ctx").unwrap();
        assert_eq!(
            decrypt(&key, &nonce, &sealed, b"ctx").unwrap(),
            b"session data"
        );
    }

    #[test]
    fn tamper_and_wrong_aad_fail() {
        let key = derive_key("pw", &new_salt()).unwrap();
        let nonce = new_nonce();
        let mut sealed = encrypt(&key, &nonce, b"data", b"ctx").unwrap();
        assert!(decrypt(&key, &nonce, &sealed, b"other").is_err());

        let bit = sealed.len() - 1;
        sealed[bit] ^= 0x01;
        assert!(decrypt(&key, &nonce, &sealed, b"ctx").is_err());
    }

    #[test]
    fn nonce_uniqueness_across_calls() {
        // Nonce reuse with the same key would be catastrophic; assert the
        // generator is not degenerate.
        let a = new_nonce();
        let b = new_nonce();
        assert_ne!(a, b);
    }

    #[test]
    fn secure_password_is_not_debug_leaking() {
        let pw = SecurePassword::new("hunter2");
        let rendered = format!("{pw:?}");
        assert!(!rendered.contains("hunter2"));
        assert_eq!(pw.as_bytes(), b"hunter2");
    }
}
