//! Build-time choice of encrypted SQLCipher or native async Turso.
//! Run `npm run configure -- --database sqlcipher|turso`; see docs/DATABASE.md.

pub mod key;
#[cfg(feature = "legacy-sqlcipher-import")]
mod legacy;
pub mod notifications;
pub mod settings;
pub mod users;
pub mod vectors;

use std::path::Path;

#[cfg(feature = "database-turso")]
mod turso_store;
#[cfg(feature = "database-turso")]
pub use turso_store::Database;
#[cfg(feature = "database-sqlcipher")]
mod sqlcipher_store;
#[cfg(feature = "database-sqlcipher")]
pub use sqlcipher_store::Database;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[cfg(feature = "database-turso")]
    #[error("database error: {0}")]
    Turso(#[from] turso::Error),
    #[cfg(any(feature = "database-sqlcipher", feature = "legacy-sqlcipher-import"))]
    #[error("SQLCipher database error: {0}")]
    Sqlcipher(rusqlite::Error),
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
    #[error("{0}")]
    Unsupported(&'static str),
    #[error("database worker failed")]
    Worker,
    #[error("not found")]
    NotFound,
}

#[cfg(any(feature = "database-sqlcipher", feature = "legacy-sqlcipher-import"))]
impl From<rusqlite::Error> for DbError {
    fn from(error: rusqlite::Error) -> Self {
        if matches!(&error, rusqlite::Error::SqliteFailure(code, _) if code.code == rusqlite::ErrorCode::NotADatabase)
        {
            Self::WrongKey
        } else {
            Self::Sqlcipher(error)
        }
    }
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

fn suffix_path(path: &Path, suffix: &str) -> std::path::PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    std::path::PathBuf::from(name)
}

pub(crate) fn remove_files(path: &Path) -> Result<(), DbError> {
    for suffix in ["", "-wal", "-shm", ".tshm"] {
        match std::fs::remove_file(suffix_path(path, suffix)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
