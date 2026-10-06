//! Credential cache (prompt 2.2): short-lived in-memory credential reuse
//! plus optional encrypted persistence ("Remember password").
//!
//! - Memory: entries hold `Zeroizing` strings; `clear()`/Drop zeroize them.
//! - Persistence: `export_encrypted`/`import_encrypted` seal the cache with
//!   AES-256-GCM under a key derived (Argon2id) from the *master password* —
//!   the same trust root as `sessions.enc`, never a machine-derived key.
//! - Log safety: `Debug` redacts everything.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use zeroize::Zeroizing;

use crate::utils::crypto;

/// Frame magic for persisted credential blobs (also the AEAD AAD).
const MAGIC: &[u8; 9] = b"MBXTCRED1";

/// One cached credential set for a session.
#[derive(Clone, Default)]
pub struct CachedCredential {
    pub password: Option<Zeroizing<String>>,
    pub passphrase: Option<Zeroizing<String>>,
    /// "Remember password" opt-in (prompt 2.2) — gates persistence.
    pub remember: bool,
}

impl std::fmt::Debug for CachedCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachedCredential")
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field(
                "passphrase",
                &self.passphrase.as_ref().map(|_| "<redacted>"),
            )
            .field("remember", &self.remember)
            .finish()
    }
}

/// In-memory credential cache shared across UI/runtime (`Clone` = same data).
#[derive(Clone, Default)]
pub struct CredentialCache {
    inner: Arc<Mutex<HashMap<String, CachedCredential>>>,
}

impl std::fmt::Debug for CredentialCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self.inner.lock().map(|m| m.len()).unwrap_or(0);
        f.debug_struct("CredentialCache")
            .field("entries", &count)
            .finish()
    }
}

impl CredentialCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Store/refresh a password for `session_name` ("Remember password").
    pub fn put_password(&self, session_name: &str, password: &str, remember: bool) {
        if let Ok(mut map) = self.inner.lock() {
            let entry = map.entry(session_name.to_string()).or_default();
            let mut wrapped = Zeroizing::new(password.to_string());
            entry.password = Some(std::mem::take(&mut wrapped));
            entry.remember = remember;
        }
    }

    /// Store/refresh a key passphrase.
    pub fn put_passphrase(&self, session_name: &str, passphrase: &str, remember: bool) {
        if let Ok(mut map) = self.inner.lock() {
            let entry = map.entry(session_name.to_string()).or_default();
            let mut wrapped = Zeroizing::new(passphrase.to_string());
            entry.passphrase = Some(std::mem::take(&mut wrapped));
            entry.remember = remember;
        }
    }

    /// Fetch a cached password (clone is short-lived and handed to the auth
    /// context, which zeroizes on drop).
    pub fn get_password(&self, session_name: &str) -> Option<String> {
        self.inner
            .lock()
            .ok()?
            .get(session_name)?
            .password
            .as_ref()
            .map(|pw| pw.as_str().to_string())
    }

    /// Fetch a cached passphrase.
    pub fn get_passphrase(&self, session_name: &str) -> Option<String> {
        self.inner
            .lock()
            .ok()?
            .get(session_name)?
            .passphrase
            .as_ref()
            .map(|p| p.as_str().to_string())
    }

    /// Forget one session's credentials (called on disconnect-by-choice or
    /// when "remember" is revoked).
    pub fn remove(&self, session_name: &str) -> bool {
        self.inner
            .lock()
            .map(|mut map| {
                if let Some(mut entry) = map.remove(session_name) {
                    wipe_entry(&mut entry);
                    true
                } else {
                    false
                }
            })
            .unwrap_or(false)
    }

    /// Forget everything (zeroizes all entries).
    pub fn clear(&self) {
        if let Ok(mut map) = self.inner.lock() {
            for (_, mut entry) in map.drain() {
                wipe_entry(&mut entry);
            }
        }
    }

    /// Number of cached sessions (diagnostics/tests).
    pub fn len(&self) -> usize {
        self.inner.lock().map(|m| m.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Only entries explicitly marked "remember" are persisted.
    pub fn persistable_count(&self) -> usize {
        self.inner
            .lock()
            .map(|m| m.values().filter(|e| e.remember).count())
            .unwrap_or(0)
    }

    /// Seal the persistable subset into an encrypted blob (master-password
    /// derived key; framing `MAGIC || nonce || ciphertext+tag`).
    pub fn export_encrypted(&self, master_password: &str) -> Result<Vec<u8>, crypto::CryptoError> {
        #[derive(serde::Serialize, serde::Deserialize)]
        struct Persisted {
            entries: Vec<(String, PersistedEntry)>,
        }
        #[derive(serde::Serialize, serde::Deserialize)]
        struct PersistedEntry {
            password: Option<String>,
            passphrase: Option<String>,
            remember: bool,
        }

        let snapshot: Vec<(String, PersistedEntry)> = match self.inner.lock() {
            Ok(map) => map
                .iter()
                .filter(|(_, entry)| entry.remember)
                .map(|(name, entry)| {
                    (
                        name.clone(),
                        PersistedEntry {
                            password: entry.password.as_ref().map(|p| p.as_str().to_string()),
                            passphrase: entry.passphrase.as_ref().map(|p| p.as_str().to_string()),
                            remember: true,
                        },
                    )
                })
                .collect(),
            Err(_) => Vec::new(),
        };

        let salt = crypto::new_salt();
        let key = crypto::derive_key(master_password, &salt)?;
        let nonce = crypto::new_nonce();
        let plaintext = rmp_serde::to_vec(&Persisted { entries: snapshot }).unwrap_or_default();
        let sealed = crypto::encrypt(&key, &nonce, &plaintext, MAGIC)?;

        let mut out =
            Vec::with_capacity(MAGIC.len() + crypto::SALT_LEN + crypto::NONCE_LEN + sealed.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&salt);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&sealed);
        Ok(out)
    }

    /// Open a blob produced by [`export_encrypted`], replacing only the
    /// `remember` entries it contains.
    pub fn import_encrypted(
        &self,
        blob: &[u8],
        master_password: &str,
    ) -> Result<usize, crypto::CryptoError> {
        #[derive(serde::Serialize, serde::Deserialize)]
        struct Persisted {
            entries: Vec<(String, PersistedEntry)>,
        }
        #[derive(serde::Serialize, serde::Deserialize)]
        struct PersistedEntry {
            password: Option<String>,
            passphrase: Option<String>,
            remember: bool,
        }

        const HEADER: usize = MAGIC.len() + crypto::SALT_LEN + crypto::NONCE_LEN;
        if blob.len() < HEADER || &blob[..MAGIC.len()] != MAGIC {
            return Err(crypto::CryptoError::Decrypt);
        }
        let salt = &blob[MAGIC.len()..MAGIC.len() + crypto::SALT_LEN];
        let nonce: &[u8; crypto::NONCE_LEN] = blob[MAGIC.len() + crypto::SALT_LEN..HEADER]
            .try_into()
            .map_err(|_| crypto::CryptoError::Decrypt)?;
        let key = crypto::derive_key(master_password, salt)?;
        let plaintext = crypto::decrypt(&key, nonce, &blob[HEADER..], MAGIC)?;
        let persisted: Persisted =
            rmp_serde::from_slice(&plaintext).map_err(|_| crypto::CryptoError::Decrypt)?;

        let mut count = 0;
        if let Ok(mut map) = self.inner.lock() {
            for (name, entry) in persisted.entries {
                let mut cached = CachedCredential {
                    remember: true,
                    ..Default::default()
                };
                if let Some(password) = entry.password {
                    let mut wrapped = Zeroizing::new(password);
                    cached.password = Some(std::mem::take(&mut wrapped));
                }
                if let Some(passphrase) = entry.passphrase {
                    let mut wrapped = Zeroizing::new(passphrase);
                    cached.passphrase = Some(std::mem::take(&mut wrapped));
                }
                map.insert(name, cached);
                count += 1;
            }
        }
        Ok(count)
    }
}

fn wipe_entry(entry: &mut CachedCredential) {
    use std::ops::DerefMut;
    use zeroize::Zeroize;
    if let Some(password) = entry.password.as_mut() {
        password.deref_mut().zeroize();
    }
    if let Some(passphrase) = entry.passphrase.as_mut() {
        passphrase.deref_mut().zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_get_round_trip() {
        let cache = CredentialCache::new();
        cache.put_password("web-1", "hunter2", true);
        assert_eq!(cache.get_password("web-1").as_deref(), Some("hunter2"));
        cache.put_passphrase("web-1", "key-pw", false);
        assert_eq!(cache.get_passphrase("web-1").as_deref(), Some("key-pw"));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn remove_and_clear_drop_entries() {
        let cache = CredentialCache::new();
        cache.put_password("a", "1", true);
        cache.put_password("b", "2", true);
        assert!(cache.remove("a"));
        assert!(!cache.remove("a"));
        assert_eq!(cache.len(), 1);
        cache.clear();
        assert!(cache.is_empty());
        assert!(cache.get_password("b").is_none());
    }

    #[test]
    fn debug_never_leaks_secrets() {
        let cache = CredentialCache::new();
        cache.put_password("web", "hunter2", true);
        let rendered = format!("{cache:?}");
        assert!(!rendered.contains("hunter2"));

        let entry = CachedCredential {
            password: Some(Zeroizing::new("hunter2".into())),
            passphrase: None,
            remember: true,
        };
        assert!(!format!("{entry:?}").contains("hunter2"));
    }

    #[test]
    fn export_import_round_trip_with_master_password() {
        let cache = CredentialCache::new();
        cache.put_password("kept", "pw-1", true);
        cache.put_password("dropped", "pw-2", false); // not remembered

        let blob = cache.export_encrypted("master-pw").unwrap();
        let fresh = CredentialCache::new();
        assert_eq!(fresh.import_encrypted(&blob, "master-pw").unwrap(), 1);
        assert_eq!(fresh.get_password("kept").as_deref(), Some("pw-1"));
        assert!(fresh.get_password("dropped").is_none());

        // Wrong master password fails cleanly.
        let other = CredentialCache::new();
        assert!(other.import_encrypted(&blob, "wrong").is_err());
    }

    #[test]
    fn corrupted_blob_is_rejected() {
        let cache = CredentialCache::new();
        let mut blob = vec![b'M', b'B', b'X', b'T', b'C', b'R', b'E', b'D', b'1'];
        blob.extend_from_slice(&[0u8; 40]);
        assert!(cache.import_encrypted(&blob, "pw").is_err());
        assert!(cache.import_encrypted(&[0u8; 10], "pw").is_err());
    }

    #[test]
    fn persistable_count_tracks_remember_flag() {
        let cache = CredentialCache::new();
        cache.put_password("a", "1", true);
        cache.put_password("b", "2", false);
        assert_eq!(cache.persistable_count(), 1);
    }
}
