//! Encrypted, local Turso store with native async queries.
//! SQLCipher files are imported by `legacy`; see docs/DATABASE.md.

pub mod key;
mod legacy;
pub(crate) use legacy::remove_files;
pub mod notifications;
pub mod settings;
pub mod users;
pub mod vectors;

use std::path::Path;
use tokio::sync::{Mutex, MutexGuard};
use turso::{Builder, Connection, EncryptionOpts};

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Turso(#[from] turso::Error),
    #[error("legacy database error: {0}")]
    Legacy(rusqlite::Error),
    #[error("database key is invalid or the encrypted file is damaged")]
    WrongKey,
    #[error("keychain error: {0}")]
    Keychain(String),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("database file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Validation(String),
    #[error("unsupported schema version {0}; refusing to modify this database")]
    SchemaVersion(i64),
    #[error("not found")]
    NotFound,
}

impl From<rusqlite::Error> for DbError {
    fn from(error: rusqlite::Error) -> Self {
        if matches!(&error, rusqlite::Error::SqliteFailure(code, _) if code.code == rusqlite::ErrorCode::NotADatabase)
        {
            Self::WrongKey
        } else {
            Self::Legacy(error)
        }
    }
}

/// Ordered migrations. Append only; migration 1 is shared with old installs.
const MIGRATIONS: &[&str] = &[
    include_str!("migrations/0001_init.sql"),
    include_str!("migrations/0002_vectors.sql"),
];

pub struct Database {
    // Keep the engine alive until its connection is dropped.
    conn: Mutex<Connection>,
    _engine: turso::Database,
}

impl Database {
    /// Open the new store, importing an old SQLCipher install when needed.
    /// The original encrypted file is retained for recovery.
    pub async fn open_or_import(path: &Path, legacy: &Path, key: &str) -> Result<Self, DbError> {
        if !path.try_exists()? && legacy.try_exists()? {
            legacy::import(path, legacy, key).await?;
        }
        Self::open(path, key).await
    }

    pub async fn open(path: &Path, key: &str) -> Result<Self, DbError> {
        key::validate(key)?;
        let path = path
            .to_str()
            .ok_or_else(|| DbError::Validation("database path is not UTF-8".into()))?;
        let engine = Builder::new_local(path)
            .experimental_encryption(true)
            .with_encryption(EncryptionOpts {
                cipher: "aegis256".into(),
                hexkey: key.into(),
            })
            .build()
            .await
            .map_err(|e| match e {
                turso::Error::NotAdb(_) => DbError::WrongKey,
                // The pinned SDK reports an authentication failure as a generic
                // error; retain the starter's stable wrong-key IPC code.
                turso::Error::Error(ref message)
                    if message.contains("decryption failed: invalid tag")
                        || message.starts_with("Decryption failed for page=") =>
                {
                    DbError::WrongKey
                }
                other => DbError::Turso(other),
            })?;
        Self::init(engine).await
    }

    /// Unencrypted and ephemeral; only for tests and tooling.
    pub async fn open_in_memory() -> Result<Self, DbError> {
        Self::init(Builder::new_local(":memory:").build().await?).await
    }

    async fn init(engine: turso::Database) -> Result<Self, DbError> {
        let mut conn = engine.connect()?;
        conn.execute("PRAGMA foreign_keys = ON", ()).await?;
        migrate(&mut conn, MIGRATIONS).await?;
        Ok(Self {
            conn: Mutex::new(conn),
            _engine: engine,
        })
    }

    /// Async serialization keeps multi-statement operations and row IDs isolated.
    /// Never hold a std::sync lock across database I/O.
    pub(crate) async fn connection(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().await
    }

    pub async fn schema_version(&self) -> Result<i64, DbError> {
        let conn = self.connection().await;
        Ok(conn
            .query("PRAGMA user_version", ())
            .await?
            .next()
            .await?
            .ok_or(turso::Error::QueryReturnedNoRows)?
            .get(0)?)
    }

    pub(crate) async fn checkpoint(&self) -> Result<(), DbError> {
        let conn = self.connection().await;
        let mut rows = conn.query("PRAGMA wal_checkpoint(TRUNCATE)", ()).await?;
        while let Some(row) = rows.next().await? {
            if row.get::<i64>(0)? != 0 {
                return Err(DbError::Validation("database checkpoint is busy".into()));
            }
        }
        Ok(())
    }
}

async fn migrate(conn: &mut Connection, migrations: &[&str]) -> Result<(), DbError> {
    let current: i64 = conn
        .query("PRAGMA user_version", ())
        .await?
        .next()
        .await?
        .ok_or(turso::Error::QueryReturnedNoRows)?
        .get(0)?;
    if current < 0 || current > migrations.len() as i64 {
        return Err(DbError::SchemaVersion(current));
    }
    for (i, sql) in migrations.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction().await?;
        tx.execute_batch(sql).await?;
        tx.execute(&format!("PRAGMA user_version = {}", i + 1), ())
            .await?;
        tx.commit().await?;
    }
    Ok(())
}

/// True when the file on disk does NOT start with the plaintext SQLite header,
/// a sanity check used after successfully opening with encryption enabled.
/// This header check alone is not proof of encryption.
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

    #[tokio::test]
    async fn migrations_are_idempotent_and_fail_atomically() {
        let db = Database::open_in_memory().await.unwrap();
        assert_eq!(db.schema_version().await.unwrap(), MIGRATIONS.len() as i64);
        {
            let mut conn = db.connection().await;
            migrate(&mut conn, MIGRATIONS).await.unwrap();
            let mut broken = MIGRATIONS.to_vec();
            broken.push("CREATE TABLE partial (id INTEGER); INSERT INTO missing_table VALUES (1);");
            assert!(migrate(&mut conn, &broken).await.is_err());
            let mut rows = conn
                .query(
                    "SELECT count(*) FROM sqlite_master WHERE name='partial'",
                    (),
                )
                .await
                .unwrap();
            assert_eq!(
                rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap(),
                0
            );
        }
        assert_eq!(db.schema_version().await.unwrap(), 2);
        let mut conn = db.connection().await;
        conn.execute("PRAGMA user_version = 999", ()).await.unwrap();
        assert!(matches!(
            migrate(&mut conn, MIGRATIONS).await,
            Err(DbError::SchemaVersion(999))
        ));
    }

    #[tokio::test]
    async fn encrypted_store_and_wal_hide_content_and_reopen() {
        let path = temp_db_path("turso");
        let key = key::generate_hex();
        let db = Database::open(&path, &key).await.unwrap();
        settings::set_raw(&db, "probe", "plaintext-marker")
            .await
            .unwrap();
        for entry in std::fs::read_dir(path.parent().unwrap()).unwrap() {
            let bytes = std::fs::read(entry.unwrap().path()).unwrap();
            assert!(!bytes.windows(16).any(|w| w == b"plaintext-marker"));
        }
        db.checkpoint().await.unwrap();
        drop(db);
        assert!(file_is_encrypted(&path));
        let wrong = Database::open(&path, &key::generate_hex()).await;
        assert!(
            matches!(wrong, Err(DbError::WrongKey)),
            "unexpected error: {:?}",
            wrong.err()
        );
        let db = Database::open(&path, &key).await.unwrap();
        assert_eq!(
            settings::get_raw(&db, "probe").await.unwrap().as_deref(),
            Some("plaintext-marker")
        );
        drop(db);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[tokio::test]
    async fn concurrent_notifications_keep_their_own_row_ids() {
        let db = std::sync::Arc::new(Database::open_in_memory().await.unwrap());
        let mut tasks = Vec::new();
        for i in 0..20 {
            let db = db.clone();
            tasks.push(tokio::spawn(async move {
                notifications::insert(
                    &db,
                    &format!("item-{i}"),
                    "",
                    notifications::NotificationKind::Info,
                )
                .await
                .unwrap()
            }));
        }
        let stored = {
            let mut returned = Vec::new();
            for task in tasks {
                returned.push(task.await.unwrap());
            }
            returned
        };
        let all = notifications::list(&db, 100).await.unwrap();
        assert_eq!(all.len(), 20);
        for item in stored {
            assert!(all.contains(&item));
        }
    }
}
