use crate::db::{Database, DbError, now_millis};
use turso::params;

pub async fn get_raw(db: &Database, key: &str) -> Result<Option<String>, DbError> {
    let conn = db.connection().await;
    let mut rows = conn
        .query("SELECT value FROM settings WHERE key = ?1", [key])
        .await?;
    Ok(rows.next().await?.map(|r| r.get(0)).transpose()?)
}

pub async fn set_raw(db: &Database, key: &str, value: &str) -> Result<(), DbError> {
    db.connection()
        .await
        .execute(
            "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![key, value, now_millis()],
        )
        .await?;
    Ok(())
}

pub async fn delete(db: &Database, key: &str) -> Result<bool, DbError> {
    Ok(db
        .connection()
        .await
        .execute("DELETE FROM settings WHERE key = ?1", [key])
        .await?
        > 0)
}
