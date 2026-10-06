//! Storage layer: session database, config store, and the crypto vault
//! (security architecture §6.1: Argon2id KDF → AES-256-GCM record envelope).
//!
//! All secret-bearing material flows through [`CryptoVault`]; nothing in
//! this crate ever writes plaintext secrets to disk.

pub mod config;
pub mod crypto;
pub mod sessions;

pub use config::{Config, ConfigStore};
pub use crypto::{CryptoError, CryptoVault};
pub use sessions::SessionStore;

/// Storage-layer errors (thiserror per-crate enum, architecture §7).
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Serialize(String),
    #[error("crypto error: {0}")]
    Crypto(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
