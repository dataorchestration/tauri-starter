use super::{DbError, key};
use std::path::Path;
use tokio::sync::{Mutex, MutexGuard};
use turso::{Builder, Connection, EncryptionOpts};

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
            #[cfg(feature = "legacy-sqlcipher-import")]
            super::legacy::import(path, legacy, key).await?;
            #[cfg(not(feature = "legacy-sqlcipher-import"))]
            return Err(DbError::Unsupported(
                "existing SQLCipher data requires --database turso --import-sqlcipher true",
            ));
        }
        Self::open(path, key).await
    }

    pub async fn open_for_app(paths: &crate::paths::AppPaths, key: &str) -> Result<Self, DbError> {
        Self::open_or_import(&paths.turso_db_file(), &paths.legacy_db_file(), key).await
    }

    pub async fn close(self) -> Result<(), DbError> {
        self.checkpoint().await
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
    #[cfg(not(feature = "legacy-sqlcipher-import"))]
    #[tokio::test]
    async fn legacy_data_requires_explicit_import_feature() {
        let root = crate::db::temp_db_path("import-disabled")
            .parent()
            .unwrap()
            .to_owned();
        let legacy = root.join("app.sqlite");
        let target = root.join("app.turso.sqlite");
        std::fs::write(&legacy, b"retained legacy data").unwrap();
        assert!(matches!(
            Database::open_or_import(&target, &legacy, &key::generate_hex()).await,
            Err(DbError::Unsupported(_))
        ));
        assert!(!target.exists());
        assert_eq!(std::fs::read(&legacy).unwrap(), b"retained legacy data");
        std::fs::remove_dir_all(root).unwrap();
    }
}
