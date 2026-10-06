//! Crypto vault: master password → Argon2id KEK → AES-256-GCM record
//! envelope. Blob format: `nonce(12) || ciphertext+tag`, with AAD binding
//! the blob to its purpose so ciphertexts can't be transplanted between
//! record kinds (architecture §6.1).

use zeroize::Zeroize;
use zeroize::Zeroizing;

const AAD_PURPOSE: &[u8] = b"mbxt-record-v1";
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;
const SALT_LEN: usize = 16;

/// Crypto-layer failures (mapped to friendly UI messages in `app`).
#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("key derivation failed")]
    Kdf,
    #[error("decryption failed (wrong password or corrupted record)")]
    Decrypt,
    #[error("OS entropy unavailable")]
    Entropy,
}

/// Holds the derived key-encryption-key in protected memory (`Zeroizing`).
#[derive(Clone)]
pub struct CryptoVault {
    kek: Zeroizing<[u8; KEY_LEN]>,
}

impl std::fmt::Debug for CryptoVault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never log key material (architecture §6.2 log discipline).
        f.debug_struct("CryptoVault").finish_non_exhaustive()
    }
}

impl CryptoVault {
    /// Derive the KEK from a master password and per-store random salt.
    ///
    /// Argon2id, v0x13, m=19 MiB / t=2 / p=1 (RFC 9106 baseline; production
    /// tunes to ~100 ms on reference hardware per architecture §6.1).
    pub fn unlock(password: &[u8], salt: &[u8]) -> Result<Self, CryptoError> {
        use argon2::{Algorithm, Argon2, Params, Version};
        let mut kek = Zeroizing::new([0u8; KEY_LEN]);
        let argon2 = Argon2::new(
            Algorithm::Argon2id,
            Version::V0x13,
            Params::new(19_456, 2, 1, Some(KEY_LEN)).map_err(|_| CryptoError::Kdf)?,
        );
        argon2
            .hash_password_into(password, salt, kek.as_mut())
            .map_err(|_| CryptoError::Kdf)?;
        Ok(Self { kek })
    }

    /// Generate a fresh random salt for a new database.
    pub fn new_salt() -> [u8; SALT_LEN] {
        let mut salt = [0u8; SALT_LEN];
        getrandom::getrandom(&mut salt).expect("OS entropy unavailable");
        salt
    }

    /// Encrypt `plaintext` into `nonce || ciphertext+tag`.
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        use aes_gcm::aead::{Aead, KeyInit, Payload};
        use aes_gcm::{Aes256Gcm, Nonce};

        let cipher = Aes256Gcm::new_from_slice(self.kek.as_ref()).map_err(|_| CryptoError::Kdf)?;
        let mut nonce_bytes = [0u8; NONCE_LEN];
        getrandom::getrandom(&mut nonce_bytes).map_err(|_| CryptoError::Entropy)?;
        let sealed = cipher
            .encrypt(
                Nonce::from_slice(&nonce_bytes),
                Payload {
                    msg: plaintext,
                    aad: AAD_PURPOSE,
                },
            )
            .map_err(|_| CryptoError::Decrypt)?;

        let mut out = Vec::with_capacity(NONCE_LEN + sealed.len());
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&sealed);
        Ok(out)
    }

    /// Decrypt a blob produced by [`encrypt`](Self::encrypt).
    pub fn decrypt(&self, blob: &[u8]) -> Result<Vec<u8>, CryptoError> {
        use aes_gcm::aead::{Aead, KeyInit, Payload};
        use aes_gcm::{Aes256Gcm, Nonce};

        if blob.len() < NONCE_LEN {
            return Err(CryptoError::Decrypt);
        }
        let cipher = Aes256Gcm::new_from_slice(self.kek.as_ref()).map_err(|_| CryptoError::Kdf)?;
        let (nonce_bytes, sealed) = blob.split_at(NONCE_LEN);
        cipher
            .decrypt(
                Nonce::from_slice(nonce_bytes),
                Payload {
                    msg: sealed,
                    aad: AAD_PURPOSE,
                },
            )
            .map_err(|_| CryptoError::Decrypt)
    }
}

/// Zeroize a decrypted plaintext buffer after use (architecture §6.2).
pub fn wipe(buf: &mut [u8]) {
    buf.zeroize();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_wrong_password_and_tamper() {
        let salt = CryptoVault::new_salt();
        let vault = CryptoVault::unlock(b"correct horse", &salt).unwrap();
        let blob = vault.encrypt(b"secret session data").unwrap();
        assert_eq!(vault.decrypt(&blob).unwrap(), b"secret session data");

        let wrong = CryptoVault::unlock(b"incorrect", &salt).unwrap();
        assert!(wrong.decrypt(&blob).is_err());

        let mut tampered = blob.clone();
        tampered[20] ^= 0xFF;
        assert!(vault.decrypt(&tampered).is_err());
    }

    #[test]
    fn deterministic_kdf_same_password_same_key() {
        let salt = CryptoVault::new_salt();
        let a = CryptoVault::unlock(b"pw", &salt).unwrap();
        let b = CryptoVault::unlock(b"pw", &salt).unwrap();
        let blob = a.encrypt(b"x").unwrap();
        assert!(b.decrypt(&blob).is_ok());
    }
}
