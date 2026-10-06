//! Session persistence: SQLite (encrypted at rest via `CryptoVault` in the
//! full implementation; the plaintext `name` index is kept for FTS search
//! per feature matrix #14).

use mbxt_core::SessionId;
use rusqlite::Connection;

/// Session database handle.
#[derive(Debug)]
pub struct SessionStore {
    conn: Connection,
}

/// Persistence failures.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Serialize(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl SessionStore {
    /// Open (creating if needed) the session database at `path`.
    pub fn open(path: &std::path::Path) -> Result<Self, StoreError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS sessions (
                 id       INTEGER PRIMARY KEY AUTOINCREMENT,
                 name     TEXT NOT NULL UNIQUE,
                 protocol TEXT NOT NULL DEFAULT 'Ssh',
                 blob     BLOB NOT NULL DEFAULT x'00'
             );
             CREATE TABLE IF NOT EXISTS history (
                 id        INTEGER PRIMARY KEY AUTOINCREMENT,
                 session   INTEGER NOT NULL REFERENCES sessions(id),
                 connected_at INTEGER NOT NULL
             );",
        )?;
        Ok(Self { conn })
    }

    /// List stored session names (sidebar/search index, #10/#14).
    pub fn list(&self) -> Result<Vec<String>, StoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT name FROM sessions ORDER BY name")?;
        let names = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(names)
    }

    /// Insert a new session placeholder; returns its id.
    pub fn add(&mut self, name: &str) -> Result<SessionId, StoreError> {
        self.conn.execute(
            "INSERT INTO sessions (name) VALUES (?1)",
            rusqlite::params![name],
        )?;
        Ok(self.conn.last_insert_rowid() as SessionId)
    }

    /// Serialize a spec into the session row's binary blob (MessagePack).
    pub fn put_blob(&mut self, id: SessionId, blob: &[u8]) -> Result<(), StoreError> {
        self.conn.execute(
            "UPDATE sessions SET blob = ?1 WHERE id = ?2",
            rusqlite::params![blob, id as i64],
        )?;
        Ok(())
    }

    /// MessagePack helper for session blobs (tech_stack §5).
    pub fn pack<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, StoreError> {
        rmp_serde::to_vec(value).map_err(|e| StoreError::Serialize(e.to_string()))
    }

    /// MessagePack helper for session blobs (tech_stack §5).
    pub fn unpack<T: serde::de::DeserializeOwned>(blob: &[u8]) -> Result<T, StoreError> {
        rmp_serde::from_slice(blob).map_err(|e| StoreError::Serialize(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_list_round_trip() {
        let path = std::env::temp_dir().join(format!("mbxt-store-test-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut store = SessionStore::open(&path).unwrap();
        let id = store.add("prod-web-01").unwrap();
        let spec = mbxt_core::SessionSpec {
            name: "prod-web-01".into(),
            protocol: mbxt_core::Protocol::Ssh,
            host: Some("10.0.0.5".into()),
            port: Some(22),
            username: Some("admin".into()),
            auth: mbxt_core::AuthMethod::Agent { forward: true },
            tags: vec!["prod".into()],
            notes: String::new(),
            x11_forwarding: false,
            serial: None,
            forwards: Vec::new(),
        };
        store
            .put_blob(id, &SessionStore::pack(&spec).unwrap())
            .unwrap();
        assert_eq!(store.list().unwrap(), vec!["prod-web-01"]);
        let _ = std::fs::remove_file(&path);
    }
}
