//! One-time, read-only SQLCipher import. Plaintext exists only in memory.
use super::{Database, DbError, key};
use rusqlite::{Connection, OpenFlags, types::ValueRef};
use std::path::{Path, PathBuf};

const TABLES: &[(&str, &str, &str)] = &[
    (
        "users",
        "id, display_name, email, avatar_path, created_at, updated_at",
        "?1, ?2, ?3, ?4, ?5, ?6",
    ),
    ("settings", "key, value, updated_at", "?1, ?2, ?3"),
    (
        "notifications",
        "id, title, body, kind, read, created_at",
        "?1, ?2, ?3, ?4, ?5, ?6",
    ),
    ("sqlite_sequence", "name, seq", "?1, ?2"),
];

type Snapshot = Vec<Vec<Vec<turso::Value>>>;

fn schema(conn: &Connection) -> Result<Vec<(String, String, Option<String>)>, DbError> {
    let mut stmt = conn.prepare("SELECT type, name, sql FROM sqlite_master ORDER BY type, name")?;
    Ok(stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?)
}

fn read_snapshot(path: &Path, key_hex: &str) -> Result<Snapshot, DbError> {
    key::validate(key_hex)?;
    let mut conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.pragma_update(None, "key", format!("x'{key_hex}'"))?;
    let tx = conn.transaction()?;
    let version: i64 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version != 1 {
        return Err(DbError::SchemaVersion(version));
    }
    // Compare the entire shipped schema, not just columns: otherwise custom
    // constraints/indexes could silently disappear in the destination.
    let expected = Connection::open_in_memory()?;
    expected.execute_batch(include_str!("migrations/0001_init.sql"))?;
    if schema(&tx)? != schema(&expected)? {
        return Err(DbError::Validation(
            "legacy database has a custom schema; supply an application-specific importer".into(),
        ));
    }
    let mut snapshot = Vec::new();
    for (table, columns, _) in TABLES {
        let mut stmt = tx.prepare(&format!("SELECT {columns} FROM {table}"))?;
        let width = stmt.column_count();
        let mut rows = stmt.query([])?;
        let mut values = Vec::new();
        while let Some(row) = rows.next()? {
            let mut record = Vec::new();
            for i in 0..width {
                record.push(match row.get_ref(i)? {
                    ValueRef::Null => turso::Value::Null,
                    ValueRef::Integer(v) => turso::Value::Integer(v),
                    ValueRef::Real(v) => turso::Value::Real(v),
                    ValueRef::Text(v) => turso::Value::Text(
                        std::str::from_utf8(v)
                            .map_err(|_| DbError::Validation("legacy text is not UTF-8".into()))?
                            .into(),
                    ),
                    ValueRef::Blob(v) => turso::Value::Blob(v.to_vec()),
                });
            }
            values.push(record);
        }
        snapshot.push(values);
    }
    tx.commit()?;
    Ok(snapshot)
}

pub(super) async fn import(destination: &Path, source: &Path, key: &str) -> Result<(), DbError> {
    let source = source.to_owned();
    let owned_key = key.to_owned();
    // The retired engine is synchronous; only this one-time read uses a worker.
    let snapshot = tokio::task::spawn_blocking(move || read_snapshot(&source, &owned_key))
        .await
        .map_err(|_| DbError::Validation("legacy import worker failed".into()))??;
    let staging = destination.with_extension("importing");
    remove_files(&staging)?; // retry after an interrupted import
    let db = Database::open(&staging, key).await?;
    {
        let mut conn = db.connection().await;
        let tx = conn.transaction().await?;
        // Explicit notification IDs recreate sqlite_sequence; copy its original
        // high-water mark as well, including IDs deleted before the migration.
        for ((table, columns, placeholders), rows) in TABLES.iter().zip(snapshot) {
            if *table == "sqlite_sequence" {
                tx.execute("DELETE FROM sqlite_sequence", ()).await?;
            }
            for row in rows {
                tx.execute(
                    format!("INSERT INTO {table} ({columns}) VALUES ({placeholders})"),
                    turso::params_from_iter(row),
                )
                .await?;
            }
        }
        tx.commit().await?;
    }
    db.checkpoint().await?;
    drop(db);
    // Publish only a complete, checkpointed database. Old SQLCipher files/key
    // remain untouched. The staging WAL must be empty before moving the main file.
    let wal = suffix_path(&staging, "-wal");
    if wal.try_exists()? && std::fs::metadata(&wal)?.len() != 0 {
        return Err(DbError::Validation(
            "import WAL was not fully checkpointed".into(),
        ));
    }
    std::fs::File::open(&staging)?.sync_all()?;
    if destination.try_exists()? {
        return Err(DbError::Validation(
            "import destination already exists".into(),
        ));
    }
    std::fs::rename(&staging, destination)?;
    #[cfg(unix)]
    if let Some(parent) = destination.parent() {
        std::fs::File::open(parent)?.sync_all()?;
    }
    remove_files(&staging)?;
    Ok(())
}

fn suffix_path(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
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
    use super::super::{notifications, settings, users};
    use super::*;

    fn fixture(path: &Path, key: &str) {
        let conn = Connection::open(path).unwrap();
        conn.pragma_update(None, "key", format!("x'{key}'"))
            .unwrap();
        conn.execute_batch(include_str!("migrations/0001_init.sql"))
            .unwrap();
        conn.execute_batch(
            "PRAGMA user_version=1;
            INSERT INTO users VALUES ('u','Ada',NULL,NULL,123,456);
            INSERT INTO settings VALUES ('custom','preserved',789);
            INSERT INTO notifications VALUES (42,'title','body','warning',1,123);
            INSERT INTO notifications VALUES (99,'deleted','','info',0,123);
            DELETE FROM notifications WHERE id=99;",
        )
        .unwrap();
    }

    #[tokio::test]
    async fn imports_without_plaintext_export_preserves_ids_and_retries_staging() {
        let source = super::super::temp_db_path("legacy");
        let target = source.with_file_name("app.turso.sqlite");
        let key = key::generate_hex();
        fixture(&source, &key);
        let before = std::fs::read(&source).unwrap();
        // Simulate an interrupted migration; it must never be adopted as final.
        let partial = Database::open(&target.with_extension("importing"), &key)
            .await
            .unwrap();
        settings::set_raw(&partial, "partial", "discard")
            .await
            .unwrap();
        drop(partial);
        let db = Database::open_or_import(&target, &source, &key)
            .await
            .unwrap();
        assert_eq!(std::fs::read(&source).unwrap(), before);
        assert_eq!(
            settings::get_raw(&db, "custom").await.unwrap().as_deref(),
            Some("preserved")
        );
        assert!(settings::get_raw(&db, "partial").await.unwrap().is_none());
        let user = users::get(&db, "u").await.unwrap().unwrap();
        assert_eq!(
            (user.created_at, user.updated_at, user.email),
            (123, 456, None)
        );
        let old = notifications::list(&db, 10).await.unwrap();
        assert_eq!(old[0].id, 42);
        assert!(old[0].read);
        let new = notifications::insert(&db, "new", "", notifications::NotificationKind::Info)
            .await
            .unwrap();
        assert_eq!(new.id, 100);
        settings::set_raw(&db, "custom", "new-value").await.unwrap();
        db.checkpoint().await.unwrap();
        drop(db);
        let db = Database::open_or_import(&target, &source, &key)
            .await
            .unwrap();
        assert_eq!(
            settings::get_raw(&db, "custom").await.unwrap().as_deref(),
            Some("new-value")
        );
        assert!(super::super::file_is_encrypted(&target));
        drop(db);
        std::fs::remove_dir_all(source.parent().unwrap()).unwrap();
    }

    #[tokio::test]
    async fn imports_committed_rows_that_are_still_in_the_legacy_wal() {
        let source = super::super::temp_db_path("legacy-wal");
        let target = source.with_file_name("app.turso.sqlite");
        let key = key::generate_hex();
        fixture(&source, &key);
        let writer = Connection::open(&source).unwrap();
        writer
            .pragma_update(None, "key", format!("x'{key}'"))
            .unwrap();
        writer.pragma_update(None, "journal_mode", "WAL").unwrap();
        writer.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
        writer
            .execute(
                "INSERT INTO settings VALUES ('wal-only','committed',999)",
                [],
            )
            .unwrap();
        let source_bytes = std::fs::read(&source).unwrap();
        let wal_path = suffix_path(&source, "-wal");
        let wal_bytes = std::fs::read(&wal_path).unwrap();
        assert!(!wal_bytes.is_empty());
        let db = Database::open_or_import(&target, &source, &key)
            .await
            .unwrap();
        assert_eq!(
            settings::get_raw(&db, "wal-only").await.unwrap().as_deref(),
            Some("committed")
        );
        assert_eq!(std::fs::read(&source).unwrap(), source_bytes);
        assert_eq!(std::fs::read(&wal_path).unwrap(), wal_bytes);
        drop(db);
        drop(writer);
        std::fs::remove_dir_all(source.parent().unwrap()).unwrap();
    }

    #[tokio::test]
    async fn failed_import_does_not_publish_or_modify_legacy() {
        let source = super::super::temp_db_path("legacy-fail");
        let target = source.with_file_name("app.turso.sqlite");
        let key = key::generate_hex();
        fixture(&source, &key);
        let before = std::fs::read(&source).unwrap();
        assert!(matches!(
            Database::open_or_import(&target, &source, &key::generate_hex()).await,
            Err(DbError::WrongKey)
        ));
        assert!(!target.exists());
        assert_eq!(std::fs::read(&source).unwrap(), before);
        let conn = Connection::open(&source).unwrap();
        conn.pragma_update(None, "key", format!("x'{key}'"))
            .unwrap();
        conn.execute("CREATE TABLE custom_data (id INTEGER)", [])
            .unwrap();
        drop(conn);
        assert!(
            Database::open_or_import(&target, &source, &key)
                .await
                .is_err()
        );
        assert!(!target.exists());
        std::fs::remove_dir_all(source.parent().unwrap()).unwrap();
    }
}
