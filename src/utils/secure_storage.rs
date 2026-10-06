//! `SecureStorage`: encrypted session store + config bootstrap
//! (prompt 1.2; security architecture §6).
//!
//! On-disk layout (both under the 0700 config directory):
//! - `config.ron`   — human-readable, non-sensitive settings (`utils::config`).
//! - `sessions.enc` — encrypted sessions: `MAGIC(9) || salt(16) || nonce(12) ||
//!   AES-256-GCM(MessagePack(Vec<SessionEntry>))`, AAD = MAGIC so blobs can't
//!   be transplanted between record kinds.
//!
//! Key lifecycle: Argon2id(password, salt) → KEK; cached in `Zeroizing`
//! memory while unlocked; `lock()` / Drop zeroize it. Files are written with
//! owner-only permissions (`0600`).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use zeroize::Zeroizing;

use super::config::{write_private, SessionEntry};
use super::crypto::{self, CryptoError, SecurePassword};

/// Encrypted store file name (inside the config directory).
pub const STORE_FILE: &str = "sessions.enc";
/// Magic bytes: version + record-kind binding (also used as AEAD AAD).
pub const MAGIC: &[u8; 9] = b"MBXTSESS1";
/// Portions of a `sessions.enc` blob returned by [`SecureStorage::unframe`].
type Unframed<'a> = (
    &'a [u8; crypto::SALT_LEN],
    &'a [u8; crypto::NONCE_LEN],
    &'a [u8],
);

/// Secure storage failures.
#[derive(Debug, thiserror::Error)]
pub enum SecureStorageError {
    #[error("storage is locked — master password required")]
    Locked,
    #[error("session store not found at {0}")]
    NotFound(PathBuf),
    #[error("corrupted store header")]
    Corrupted,
    #[error("wrong master password or corrupted data")]
    AuthFailed,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serialize(String),
    #[error("crypto error: {0}")]
    Crypto(#[from] CryptoError),
    #[error("keyring error: {0}")]
    Keyring(#[from] crate::utils::keyring_bridge::KeyringBridgeError),
}

/// Thread-safe secure store handle (`Clone` shares the cached key).
#[derive(Clone)]
pub struct SecureStorage {
    inner: Arc<Inner>,
}

struct Inner {
    config_dir: PathBuf,
    /// KDF salt; read from the store header when present, else fresh
    /// (persisted on first save).
    salt: Mutex<[u8; crypto::SALT_LEN]>,
    /// Cached KEK while unlocked. Zeroized on `lock()` and on drop.
    cached_key: Mutex<Option<Zeroizing<[u8; crypto::KEY_LEN]>>>,
}

impl std::fmt::Debug for SecureStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never render salt/key material (security §6.2).
        f.debug_struct("SecureStorage")
            .field("config_dir", &self.inner.config_dir)
            .finish_non_exhaustive()
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        // Defense in depth: Zeroizing would handle this on its own, but be
        // explicit about the KEK lifetime (quality bar: no plaintext key
        // material survives the store).
        if let Ok(mut cached) = self.cached_key.lock() {
            if let Some(key) = cached.as_mut() {
                crypto::wipe(key.as_mut());
            }
            *cached = None;
        }
    }
}

impl SecureStorage {
    /// Initialize the store handle: loads the salt from an existing
    /// `sessions.enc` (if any) or generates a fresh one. Also ensures the
    /// default `config.ron` exists (created with defaults on first run).
    pub fn new(config_dir: &Path) -> Self {
        let salt = read_header_salt(&config_dir.join(STORE_FILE)).unwrap_or_else(crypto::new_salt);

        // First-run: create a default config if absent (prompt 1.2 spec).
        let config_path = config_dir.join("config.ron");
        if !config_path.exists() {
            let default = super::config::AppConfig::default();
            if let Err(err) = super::config::save(config_dir, &default) {
                tracing::warn!(%err, "could not write default config");
            }
        }

        Self {
            inner: Arc::new(Inner {
                config_dir: config_dir.to_path_buf(),
                salt: Mutex::new(salt),
                cached_key: Mutex::new(None),
            }),
        }
    }

    /// Path of the encrypted session store.
    pub fn store_path(&self) -> PathBuf {
        self.inner.config_dir.join(STORE_FILE)
    }

    /// `true` when an encrypted store exists on disk.
    pub fn has_store(&self) -> bool {
        self.store_path().exists()
    }

    /// `true` while a KEK is cached (store unlocked).
    pub fn is_unlocked(&self) -> bool {
        self.inner
            .cached_key
            .lock()
            .map(|k| k.is_some())
            .unwrap_or(false)
    }

    /// Drop the cached KEK (zeroized). Sessions become inaccessible until
    /// the next successful unlock.
    pub fn lock(&self) {
        if let Ok(mut cached) = self.inner.cached_key.lock() {
            if let Some(key) = cached.as_mut() {
                crypto::wipe(key.as_mut());
            }
            *cached = None;
        }
    }

    /// Argon2id key derivation (m=64 MiB, t=3, p=1) — exposed for tests and
    /// the CLI `--master-password-prompt` verification path.
    pub fn derive_key(
        &self,
        password: &str,
        salt: &[u8],
    ) -> Result<[u8; crypto::KEY_LEN], SecureStorageError> {
        crypto::derive_key(password, salt).map_err(Into::into)
    }

    // -- record-level (explicit password) --------------------------------

    /// Encrypt `sessions` with `password` (fresh random nonce; salt from the
    /// current header). Returns full `sessions.enc` file bytes.
    pub fn encrypt_sessions(
        &self,
        sessions: &[SessionEntry],
        password: &str,
    ) -> Result<Vec<u8>, SecureStorageError> {
        let salt = *self
            .inner
            .salt
            .lock()
            .map_err(|_| SecureStorageError::Corrupted)?;
        let mut key = crypto::derive_key(password, &salt)?;
        let nonce = crypto::new_nonce();
        let plaintext = Zeroizing::new(
            rmp_serde::to_vec(sessions)
                .map_err(|e| SecureStorageError::Serialize(e.to_string()))?,
        );
        let sealed = crypto::encrypt(&key, &nonce, &plaintext, MAGIC);
        crypto::wipe(&mut key);
        Ok(Self::frame(&salt, &nonce, &sealed?))
    }

    /// Decrypt full `sessions.enc` file bytes with `password`.
    pub fn decrypt_sessions(
        &self,
        data: &[u8],
        password: &str,
    ) -> Result<Vec<SessionEntry>, SecureStorageError> {
        let (salt, nonce, sealed) = Self::unframe(data)?;
        let mut key = crypto::derive_key(password, salt)?;
        let plaintext = crypto::decrypt(&key, nonce, sealed, MAGIC);
        crypto::wipe(&mut key);
        let plaintext = Zeroizing::new(plaintext?);
        let sessions: Vec<SessionEntry> = rmp_serde::from_slice(&plaintext)
            .map_err(|e| SecureStorageError::Serialize(e.to_string()))?;
        Ok(sessions)
    }

    // -- store-level (password or cached key) ----------------------------

    /// Load sessions, decrypting with `password` when provided (and caching
    /// the derived KEK on success), otherwise with the cached key.
    pub fn load_sessions(
        &self,
        password: Option<&str>,
    ) -> Result<Vec<SessionEntry>, SecureStorageError> {
        let data = std::fs::read(self.store_path()).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                SecureStorageError::NotFound(self.store_path())
            } else {
                e.into()
            }
        })?;

        match password {
            Some(password) => {
                let sessions = self.decrypt_sessions(&data, password)?;
                self.cache_key(password)?;
                Ok(sessions)
            },
            None => {
                let key = self.cached_key()?.ok_or(SecureStorageError::Locked)?;
                let (_salt, nonce, sealed) = Self::unframe(&data)?;
                let plaintext = Zeroizing::new(crypto::decrypt(&key, nonce, sealed, MAGIC)?);
                rmp_serde::from_slice(&plaintext)
                    .map_err(|e| SecureStorageError::Serialize(e.to_string()))
            },
        }
    }

    /// Save sessions. Uses the provided `password` when given, otherwise the
    /// cached KEK (fails with [`SecureStorageError::Locked`] when locked).
    pub fn save_sessions(
        &self,
        sessions: &[SessionEntry],
        password: Option<&str>,
    ) -> Result<(), SecureStorageError> {
        let blob = match password {
            Some(password) => self.encrypt_sessions(sessions, password)?,
            None => {
                let key = self.cached_key()?.ok_or(SecureStorageError::Locked)?;
                let salt = *self
                    .inner
                    .salt
                    .lock()
                    .map_err(|_| SecureStorageError::Corrupted)?;
                let nonce = crypto::new_nonce();
                let plaintext = Zeroizing::new(
                    rmp_serde::to_vec(sessions)
                        .map_err(|e| SecureStorageError::Serialize(e.to_string()))?,
                );
                let sealed = crypto::encrypt(&key, &nonce, &plaintext, MAGIC)?;
                Self::frame(&salt, &nonce, &sealed)
            },
        };
        write_private(self.store_path(), &blob)?;
        if let Some(password) = password {
            self.cache_key(password)?;
        }
        Ok(())
    }

    /// Re-key the store: decrypt with `old`, derive a fresh salt + key for
    /// `new`, rewrite the file, and refresh the cached KEK.
    pub fn change_master_password(&self, old: &str, new: &str) -> Result<(), SecureStorageError> {
        let data = std::fs::read(self.store_path())?;
        let sessions = self.decrypt_sessions(&data, old)?;

        let new_salt = crypto::new_salt();
        let new_key = Zeroizing::new(crypto::derive_key(new, &new_salt)?);
        let nonce = crypto::new_nonce();
        let plaintext = Zeroizing::new(
            rmp_serde::to_vec(&sessions)
                .map_err(|e| SecureStorageError::Serialize(e.to_string()))?,
        );
        let sealed = crypto::encrypt(&new_key, &nonce, &plaintext, MAGIC)?;
        write_private(self.store_path(), &Self::frame(&new_salt, &nonce, &sealed))?;

        *self
            .inner
            .salt
            .lock()
            .map_err(|_| SecureStorageError::Corrupted)? = new_salt;
        let mut zero_old = new_key;
        *self
            .inner
            .cached_key
            .lock()
            .map_err(|_| SecureStorageError::Corrupted)? = Some(std::mem::take(&mut zero_old));
        Ok(())
    }

    // -- import / export (CLI support) ------------------------------------

    /// Export sessions to `path` encrypted with `password`.
    pub fn export_sessions(
        &self,
        path: &Path,
        sessions: &[SessionEntry],
        password: &str,
    ) -> Result<(), SecureStorageError> {
        // Reuse the framing but honor the caller's file location.
        let salt = *self
            .inner
            .salt
            .lock()
            .map_err(|_| SecureStorageError::Corrupted)?;
        let mut key = crypto::derive_key(password, &salt)?;
        let nonce = crypto::new_nonce();
        let plaintext = rmp_serde::to_vec(sessions)
            .map_err(|e| SecureStorageError::Serialize(e.to_string()))?;
        let sealed = crypto::encrypt(&key, &nonce, &plaintext, MAGIC);
        crypto::wipe(&mut key);
        write_private(path.to_path_buf(), &Self::frame(&salt, &nonce, &sealed?))?;
        Ok(())
    }

    /// Import sessions from an encrypted `path`; returns the decrypted list
    /// (caller decides merge policy and persists via `save_sessions`).
    pub fn import_sessions(
        &self,
        path: &Path,
        password: &str,
    ) -> Result<Vec<SessionEntry>, SecureStorageError> {
        let data = std::fs::read(path)?;
        self.decrypt_sessions(&data, password)
    }

    // -- keyring opt-in (#49) ---------------------------------------------

    /// Cache the master password in the desktop keyring (user opt-in).
    pub fn store_password_in_keyring(&self, password: &str) -> Result<(), SecureStorageError> {
        crate::utils::keyring_bridge::store_master_password(password)?;
        Ok(())
    }

    /// Read the cached master password from the keyring, if present.
    pub fn load_password_from_keyring(&self) -> Result<Option<String>, SecureStorageError> {
        Ok(crate::utils::keyring_bridge::load_master_password()?)
    }

    // -- internals ---------------------------------------------------------

    fn frame(
        salt: &[u8; crypto::SALT_LEN],
        nonce: &[u8; crypto::NONCE_LEN],
        sealed: &[u8],
    ) -> Vec<u8> {
        let mut out = Vec::with_capacity(MAGIC.len() + salt.len() + nonce.len() + sealed.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(salt);
        out.extend_from_slice(nonce);
        out.extend_from_slice(sealed);
        out
    }

    fn unframe(data: &[u8]) -> Result<Unframed<'_>, SecureStorageError> {
        let header = MAGIC.len() + crypto::SALT_LEN + crypto::NONCE_LEN;
        if data.len() < header || &data[..MAGIC.len()] != MAGIC {
            return Err(SecureStorageError::Corrupted);
        }
        let salt = data[MAGIC.len()..MAGIC.len() + crypto::SALT_LEN]
            .try_into()
            .map_err(|_| SecureStorageError::Corrupted)?;
        let nonce = data[MAGIC.len() + crypto::SALT_LEN..header]
            .try_into()
            .map_err(|_| SecureStorageError::Corrupted)?;
        let sealed = &data[header..];
        Ok((salt, nonce, sealed))
    }

    fn cached_key(&self) -> Result<Option<Zeroizing<[u8; crypto::KEY_LEN]>>, SecureStorageError> {
        self.inner
            .cached_key
            .lock()
            .map(|k| k.clone())
            .map_err(|_| SecureStorageError::Corrupted)
    }

    /// Derive and cache the KEK for `password` (called after a successful
    /// password-based decrypt so subsequent saves need no password).
    fn cache_key(&self, password: &str) -> Result<(), SecureStorageError> {
        let salt = *self
            .inner
            .salt
            .lock()
            .map_err(|_| SecureStorageError::Corrupted)?;
        let key = crypto::derive_key(password, &salt)?;
        let mut zero = Zeroizing::new(key);
        *self
            .inner
            .cached_key
            .lock()
            .map_err(|_| SecureStorageError::Corrupted)? = Some(std::mem::take(&mut zero));
        Ok(())
    }
}

/// Read just the salt from an existing store file (None if absent/invalid).
fn read_header_salt(path: &Path) -> Option<[u8; crypto::SALT_LEN]> {
    let data = std::fs::read(path).ok()?;
    let header = MAGIC.len() + crypto::SALT_LEN;
    if data.len() < header || &data[..MAGIC.len()] != MAGIC {
        return None;
    }
    let mut salt = [0u8; crypto::SALT_LEN];
    salt.copy_from_slice(&data[MAGIC.len()..header]);
    Some(salt)
}

/// Read the whole store file, validating the magic (helper for tests/CLI).
#[allow(dead_code)]
fn read_store(path: &Path) -> Result<Vec<u8>, SecureStorageError> {
    let data = std::fs::read(path)?;
    if data.len() < MAGIC.len() + crypto::SALT_LEN + crypto::NONCE_LEN
        || &data[..MAGIC.len()] != MAGIC
    {
        return Err(SecureStorageError::Corrupted);
    }
    Ok(data)
}

// Silence while the SQLite index path (prompt 1.3+) consumes these.
#[allow(dead_code)]
fn _typecheck(pw: &SecurePassword) -> &[u8] {
    pw.as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::config::SessionEntry;
    use mbxt_core::{AuthMethod, Protocol};

    fn entry(name: &str, id: u64) -> SessionEntry {
        SessionEntry {
            id,
            name: name.to_string(),
            protocol: Protocol::Ssh,
            host: Some("10.0.0.9".into()),
            port: Some(22),
            username: Some("ops".into()),
            auth: AuthMethod::Password,
            tags: vec!["test".into()],
            notes: String::new(),
            x11_forwarding: false,
            serial: None,
            forwards: Vec::new(),
            created_secs: 1_700_000_000,
        }
    }

    fn store(dir: &Path) -> SecureStorage {
        SecureStorage::new(dir)
    }

    #[test]
    fn first_run_creates_default_config() {
        let dir = tempfile::tempdir().unwrap();
        let _ = store(dir.path());
        assert!(dir.path().join("config.ron").exists());
    }

    #[test]
    fn save_load_round_trip_with_password_then_cached_key() {
        let dir = tempfile::tempdir().unwrap();
        let storage = store(dir.path());
        let sessions = vec![entry("a", 1), entry("b", 2)];

        // Explicit password save + load (caches the key on success).
        storage.save_sessions(&sessions, Some("pass-1")).unwrap();
        assert!(storage.is_unlocked());
        assert_eq!(
            storage.load_sessions(None).unwrap().len(),
            2,
            "cached key path"
        );

        // Fresh handle (no cached key) with the right password.
        let fresh = store(dir.path());
        assert!(!fresh.is_unlocked());
        assert_eq!(fresh.load_sessions(Some("pass-1")).unwrap().len(), 2);
    }

    #[test]
    fn locked_store_rejects_passwordless_load() {
        let dir = tempfile::tempdir().unwrap();
        let storage = store(dir.path());
        storage.save_sessions(&[entry("a", 1)], Some("pw")).unwrap();
        storage.lock();
        assert!(!storage.is_unlocked());
        match storage.load_sessions(None) {
            Err(SecureStorageError::Locked) => {},
            other => panic!("expected Locked, got {other:?}"),
        }
    }

    #[test]
    fn wrong_password_fails_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let storage = store(dir.path());
        storage
            .save_sessions(&[entry("a", 1)], Some("right"))
            .unwrap();
        let fresh = store(dir.path());
        match fresh.load_sessions(Some("wrong")) {
            Err(SecureStorageError::Crypto(CryptoError::Decrypt)) => {},
            other => panic!("expected AuthFailed, got {other:?}"),
        }
    }

    #[test]
    fn missing_store_reports_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let storage = store(dir.path());
        match storage.load_sessions(Some("pw")) {
            Err(SecureStorageError::NotFound(_)) => {},
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[test]
    fn change_master_password_rekeys_and_rotates_salt() {
        let dir = tempfile::tempdir().unwrap();
        let storage = store(dir.path());
        let sessions = vec![entry("web", 1)];

        storage.save_sessions(&sessions, Some("old-pw")).unwrap();
        let old_salt = read_header_salt(&storage.store_path()).unwrap();
        storage.change_master_password("old-pw", "new-pw").unwrap();
        let new_salt = read_header_salt(&storage.store_path()).unwrap();
        assert_ne!(old_salt, new_salt, "re-key must rotate the salt");

        // Old password no longer works; new one does; cache refreshed.
        let fresh = store(dir.path());
        assert!(fresh.load_sessions(Some("old-pw")).is_err());
        assert_eq!(fresh.load_sessions(Some("new-pw")).unwrap().len(), 1);
        assert!(storage.load_sessions(None).is_ok());
    }

    #[test]
    fn export_import_round_trip_to_arbitrary_path() {
        let dir = tempfile::tempdir().unwrap();
        let storage = store(dir.path());
        let sessions = vec![entry("jump", 7)];

        let export_path = dir.path().join("export.enc");
        storage
            .export_sessions(&export_path, &sessions, "export-pw")
            .unwrap();

        let imported = storage.import_sessions(&export_path, "export-pw").unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].name, "jump");
        assert_eq!(imported[0].username.as_deref(), Some("ops"));

        // Wrong password on import fails.
        assert!(storage.import_sessions(&export_path, "nope").is_err());
    }

    #[test]
    fn corrupted_header_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        let storage = store(dir.path());
        storage.save_sessions(&[entry("a", 1)], Some("pw")).unwrap();

        let mut data = std::fs::read(storage.store_path()).unwrap();
        data[0] = b'X'; // break the magic
        std::fs::write(storage.store_path(), &data).unwrap();

        match storage.load_sessions(Some("pw")) {
            Err(SecureStorageError::Corrupted) => {},
            other => panic!("expected Corrupted, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn store_file_has_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let storage = store(dir.path());
        storage.save_sessions(&[entry("a", 1)], Some("pw")).unwrap();
        let mode = std::fs::metadata(storage.store_path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn debug_never_leaks_key_material() {
        let dir = tempfile::tempdir().unwrap();
        let storage = store(dir.path());
        storage
            .save_sessions(&[entry("a", 1)], Some("sekrit"))
            .unwrap();
        let rendered = format!("{storage:?}");
        assert!(!rendered.contains("sekrit"));
    }
}
