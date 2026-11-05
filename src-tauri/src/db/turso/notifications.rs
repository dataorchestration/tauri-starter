use super::*;
use crate::db::{Database, DbError, now_millis};
use turso::{Row, params};

fn row_to_notification(r: &Row) -> turso::Result<Notification> {
    Ok(Notification {
        id: r.get(0)?,
        title: r.get(1)?,
        body: r.get(2)?,
        kind: NotificationKind::parse(&r.get::<String>(3)?),
        read: r.get::<i64>(4)? != 0,
        created_at: r.get(5)?,
    })
}

const COLS: &str = "id, title, body, kind, read, created_at";

pub async fn insert(
    db: &Database,
    title: &str,
    body: &str,
    kind: NotificationKind,
) -> Result<Notification, DbError> {
    let now = now_millis();
    let conn = db.connection().await;
    conn.execute(
        "INSERT INTO notifications (title, body, kind, read, created_at) VALUES (?1, ?2, ?3, 0, ?4)",
        params![title, body, kind.as_str(), now],
    ).await?;
    let id = conn.last_insert_rowid();
    Ok(Notification {
        id,
        title: title.into(),
        body: body.into(),
        kind,
        read: false,
        created_at: now,
    })
}

/// Newest first.
pub async fn list(db: &Database, limit: u32) -> Result<Vec<Notification>, DbError> {
    let conn = db.connection().await;
    let mut rows = conn
        .query(
            format!("SELECT {COLS} FROM notifications ORDER BY created_at DESC, id DESC LIMIT ?1"),
            [i64::from(limit)],
        )
        .await?;
    let mut notifications = Vec::new();
    while let Some(row) = rows.next().await? {
        notifications.push(row_to_notification(&row)?);
    }
    Ok(notifications)
}

pub async fn unread_count(db: &Database) -> Result<i64, DbError> {
    let conn = db.connection().await;
    Ok(conn
        .query("SELECT count(*) FROM notifications WHERE read = 0", ())
        .await?
        .next()
        .await?
        .ok_or(turso::Error::QueryReturnedNoRows)?
        .get(0)?)
}

pub async fn mark_read(db: &Database, id: i64, read: bool) -> Result<bool, DbError> {
    Ok(db
        .connection()
        .await
        .execute(
            "UPDATE notifications SET read = ?2 WHERE id = ?1",
            params![id, read as i64],
        )
        .await?
        > 0)
}

pub async fn mark_all_read(db: &Database) -> Result<u64, DbError> {
    Ok(db
        .connection()
        .await
        .execute("UPDATE notifications SET read = 1 WHERE read = 0", ())
        .await?)
}

pub async fn delete(db: &Database, id: i64) -> Result<bool, DbError> {
    Ok(db
        .connection()
        .await
        .execute("DELETE FROM notifications WHERE id = ?1", [id])
        .await?
        > 0)
}

pub async fn clear(db: &Database) -> Result<u64, DbError> {
    Ok(db
        .connection()
        .await
        .execute("DELETE FROM notifications", ())
        .await?)
}
