use crate::db::{Database, DbError, now_millis};
use rusqlite::{OptionalExtension, params};

pub async fn get_raw(db: &Database, key: &str) -> Result<Option<String>, DbError> {
    let key = key.to_owned();
    db.with(move |c| {
        c.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .optional()
    })
    .await
}

pub async fn set_raw(db: &Database, key: &str, value: &str) -> Result<(), DbError> {
    let key = key.to_owned();
    let value = value.to_owned();
    db.with(move |c| {
        c.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![key, value, now_millis()],
        )
    }).await?;
    Ok(())
}

pub async fn delete(db: &Database, key: &str) -> Result<bool, DbError> {
    let key = key.to_owned();
    Ok(db
        .with(move |c| c.execute("DELETE FROM settings WHERE key = ?1", [key]))
        .await?
        > 0)
}
