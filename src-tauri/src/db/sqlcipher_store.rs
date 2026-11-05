//! Stable encrypted backend. SQLCipher is synchronous; work runs on the
//! blocking pool while the command API remains async.
use super::{DbError, key};
use rusqlite::Connection;
use std::{path::Path, sync::Arc};
use tokio::sync::Mutex;

const MIGRATIONS: &[&str] = &[include_str!("migrations/0001_init.sql")];

pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    pub async fn open_for_app(paths: &crate::paths::AppPaths, key: &str) -> Result<Self, DbError> {
        if paths.turso_db_file().try_exists()? {
            return Err(DbError::Unsupported(
                "Turso data already exists; use the Turso build. Switching back would reopen a stale SQLCipher snapshot.",
            ));
        }
        Self::open(&paths.legacy_db_file(), key).await
    }

    pub async fn open(path: &Path, key_hex: &str) -> Result<Self, DbError> {
        key::validate(key_hex)?;
        let path = path.to_owned();
        let key_hex = key_hex.to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = Connection::open(path)?;
            conn.pragma_update(None, "key", format!("x'{key_hex}'"))?;
            Self::init(conn)
        })
        .await
        .map_err(|_| DbError::Worker)?
    }

    pub async fn open_in_memory() -> Result<Self, DbError> {
        tokio::task::spawn_blocking(|| Self::init(Connection::open_in_memory()?))
            .await
            .map_err(|_| DbError::Worker)?
    }

    fn init(mut conn: Connection) -> Result<Self, DbError> {
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
            r.get::<_, i64>(0)
        })?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrate(&mut conn, MIGRATIONS)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub(crate) async fn with<T, F>(&self, f: F) -> Result<T, DbError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> rusqlite::Result<T> + Send + 'static,
    {
        // Acquire before spawning, and move the owned guard into the worker.
        // Cancellation of the caller cannot let reset close/delete the store
        // while the non-cancellable blocking operation is still running.
        let mut guard = self.conn.clone().lock_owned().await;
        tokio::task::spawn_blocking(move || f(&mut guard))
            .await
            .map_err(|_| DbError::Worker)?
            .map_err(Into::into)
    }

    pub async fn schema_version(&self) -> Result<i64, DbError> {
        self.with(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
            .await
    }

    pub(crate) async fn checkpoint(&self) -> Result<(), DbError> {
        let busy: i64 = self
            .with(|c| c.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0)))
            .await?;
        if busy != 0 {
            return Err(DbError::Validation("database checkpoint is busy".into()));
        }
        Ok(())
    }

    pub async fn close(self) -> Result<(), DbError> {
        // Also waits for a blocking worker whose caller has been cancelled.
        self.checkpoint().await
    }
}

fn migrate(conn: &mut Connection, migrations: &[&str]) -> Result<(), DbError> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if current < 0 || current > migrations.len() as i64 {
        return Err(DbError::SchemaVersion(current));
    }
    for (i, sql) in migrations.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", i as i64 + 1)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_atomic_and_reject_future_versions() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn, MIGRATIONS).unwrap();
        migrate(&mut conn, MIGRATIONS).unwrap();
        let broken = [
            MIGRATIONS[0],
            "CREATE TABLE partial (id INTEGER); INSERT INTO missing VALUES (1)",
        ];
        assert!(migrate(&mut conn, &broken).is_err());
        assert_eq!(
            conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            conn.query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='partial'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        conn.pragma_update(None, "user_version", 999).unwrap();
        assert!(matches!(
            migrate(&mut conn, MIGRATIONS),
            Err(DbError::SchemaVersion(999))
        ));
    }

    #[tokio::test]
    async fn blocks_switching_back_to_a_stale_snapshot() {
        let root = super::super::temp_db_path("switch")
            .parent()
            .unwrap()
            .to_owned();
        let paths = crate::paths::AppPaths::in_dirs(root.clone(), root.join("logs")).unwrap();
        std::fs::write(paths.turso_db_file(), b"existing data").unwrap();
        assert!(matches!(
            Database::open_for_app(&paths, &key::generate_hex()).await,
            Err(DbError::Unsupported(_))
        ));
        assert!(!paths.legacy_db_file().exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn cancelling_a_caller_keeps_the_worker_locked_until_completion() {
        let db = Arc::new(Database::open_in_memory().await.unwrap());
        let (started, entered) = tokio::sync::oneshot::channel();
        let (release, wait) = std::sync::mpsc::channel();
        let caller = {
            let db = db.clone();
            tokio::spawn(async move {
                db.with(move |conn| {
                    started.send(()).unwrap();
                    wait.recv().unwrap();
                    conn.execute("INSERT INTO settings VALUES ('worker','finished',1)", [])
                })
                .await
            })
        };
        entered.await.unwrap();
        caller.abort();
        let _ = caller.await;
        assert!(db.conn.try_lock().is_err());
        release.send(()).unwrap();
        assert_eq!(
            super::super::settings::get_raw(&db, "worker")
                .await
                .unwrap()
                .as_deref(),
            Some("finished")
        );
        Arc::try_unwrap(db).ok().unwrap().close().await.unwrap();
    }
}
