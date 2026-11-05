//! Encrypted local store.
//!
//! One SQLCipher database (`app.sqlite` under the app data dir) encrypted with
//! a random 256-bit key held in the OS keychain ([`key`]). The file is
//! unreadable if copied off the machine or opened by another account.
//! Migrations are plain SQL files applied in order and tracked with
//! `PRAGMA user_version` — no extra dependency.

pub mod key;
pub mod notifications;
pub mod settings;
pub mod users;

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("database is encrypted with a different key")]
    WrongKey,
    #[error("keychain error: {0}")]
    Keychain(String),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("not found")]
    NotFound,
}

/// Ordered migrations. Append only.
const MIGRATIONS: &[&str] = &[include_str!("migrations/0001_init.sql")];

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Open (creating if needed) the encrypted database at `path` with `key_hex`
    /// (64 hex chars = 32 bytes, see [`key::generate_hex`]).
    pub fn open(path: &Path, key_hex: &str) -> Result<Self, DbError> {
        let conn = Connection::open(path)?;
        Self::init(conn, Some(key_hex))
    }

    /// Unencrypted in-memory database for tests and tooling.
    pub fn open_in_memory() -> Result<Self, DbError> {
        Self::init(Connection::open_in_memory()?, None)
    }

    fn init(conn: Connection, key_hex: Option<&str>) -> Result<Self, DbError> {
        if let Some(k) = key_hex {
            // Raw-key form: SQLCipher uses the bytes as-is (no KDF), which is
            // what we want for a random key.
            conn.pragma_update(None, "key", format!("x'{k}'"))?;
        }
        // The first real read fails with NotADatabase when the key is wrong.
        match conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
            r.get::<_, i64>(0)
        }) {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == rusqlite::ErrorCode::NotADatabase =>
            {
                return Err(DbError::WrongKey);
            }
            Err(e) => return Err(e.into()),
        }
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Run `f` with the connection. The lock is held for the closure only, so
    /// keep transactions inside it.
    pub fn with<T>(
        &self,
        f: impl FnOnce(&Connection) -> rusqlite::Result<T>,
    ) -> Result<T, DbError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        Ok(f(&conn)?)
    }

    /// Schema version currently applied.
    pub fn schema_version(&self) -> Result<i64, DbError> {
        self.with(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
    }
}

/// Apply any migrations newer than `PRAGMA user_version`, each in its own
/// transaction so a failure leaves the schema at a known version.
fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate() {
        let version = i as i64 + 1;
        if version <= current {
            continue;
        }
        conn.execute_batch("BEGIN")?;
        let applied = conn
            .execute_batch(sql)
            .and_then(|_| conn.pragma_update(None, "user_version", version));
        match applied {
            Ok(()) => conn.execute_batch("COMMIT")?,
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                return Err(e);
            }
        }
    }
    Ok(())
}

/// True when the file on disk does NOT start with the plaintext SQLite header,
/// i.e. it is encrypted. Used by the health check.
pub fn file_is_encrypted(path: &Path) -> bool {
    use std::io::Read;
    let mut magic = [0u8; 16];
    match std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut magic)) {
        Ok(()) => &magic != b"SQLite format 3\0",
        Err(_) => false,
    }
}

/// Milliseconds since the Unix epoch; the timestamp used in every table.
pub fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
pub(crate) fn temp_db_path(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("starter-db-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("app.sqlite")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_apply_once_and_are_idempotent() {
        let db = Database::open_in_memory().unwrap();
        assert_eq!(db.schema_version().unwrap(), MIGRATIONS.len() as i64);
        db.with(migrate).unwrap();
        assert_eq!(db.schema_version().unwrap(), MIGRATIONS.len() as i64);
    }

    #[test]
    fn encrypted_file_rejects_wrong_key_and_hides_plaintext() {
        let path = temp_db_path("enc");
        let k1 = key::generate_hex();
        let k2 = key::generate_hex();
        {
            let db = Database::open(&path, &k1).unwrap();
            db.with(|c| {
                c.execute(
                    "INSERT INTO settings (key, value, updated_at) VALUES ('probe', 'plaintext-marker', 1)",
                    [],
                )
            })
            .unwrap();
        }
        assert!(
            file_is_encrypted(&path),
            "header must not be plaintext SQLite"
        );
        let bytes = std::fs::read(&path).unwrap();
        assert!(
            !bytes.windows(16).any(|w| w == b"plaintext-marker"),
            "row content must not appear in the file"
        );
        assert!(matches!(Database::open(&path, &k2), Err(DbError::WrongKey)));
        // Reopen with the right key still works.
        let db = Database::open(&path, &k1).unwrap();
        let v: String = db
            .with(|c| {
                c.query_row("SELECT value FROM settings WHERE key='probe'", [], |r| {
                    r.get(0)
                })
            })
            .unwrap();
        assert_eq!(v, "plaintext-marker");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
