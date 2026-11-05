use super::*;
use crate::db::{Database, DbError, now_millis};
use rusqlite::{Row, params};

fn row_to_notification(r: &Row) -> rusqlite::Result<Notification> {
    Ok(Notification {
        id: r.get(0)?,
        title: r.get(1)?,
        body: r.get(2)?,
        kind: NotificationKind::parse(&r.get::<_, String>(3)?),
        read: r.get::<_, i64>(4)? != 0,
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
    let stored_title = title.to_owned();
    let stored_body = body.to_owned();
    let id = db.with(move |c| {
        c.execute(
            "INSERT INTO notifications (title, body, kind, read, created_at) VALUES (?1, ?2, ?3, 0, ?4)",
            params![stored_title, stored_body, kind.as_str(), now],
        )?;
        Ok(c.last_insert_rowid())
    }).await?;
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
    db.with(move |c| {
        let mut st = c.prepare(&format!(
            "SELECT {COLS} FROM notifications ORDER BY created_at DESC, id DESC LIMIT ?1"
        ))?;
        st.query_map([limit], row_to_notification)?.collect()
    })
    .await
}

pub async fn unread_count(db: &Database) -> Result<i64, DbError> {
    db.with(move |c| {
        c.query_row(
            "SELECT count(*) FROM notifications WHERE read = 0",
            [],
            |r| r.get(0),
        )
    })
    .await
}

pub async fn mark_read(db: &Database, id: i64, read: bool) -> Result<bool, DbError> {
    Ok(db
        .with(move |c| {
            c.execute(
                "UPDATE notifications SET read = ?2 WHERE id = ?1",
                params![id, read as i64],
            )
        })
        .await?
        > 0)
}

pub async fn mark_all_read(db: &Database) -> Result<u64, DbError> {
    Ok(db
        .with(move |c| c.execute("UPDATE notifications SET read = 1 WHERE read = 0", []))
        .await? as u64)
}

pub async fn delete(db: &Database, id: i64) -> Result<bool, DbError> {
    Ok(db
        .with(move |c| c.execute("DELETE FROM notifications WHERE id = ?1", [id]))
        .await?
        > 0)
}

pub async fn clear(db: &Database) -> Result<u64, DbError> {
    Ok(db
        .with(move |c| c.execute("DELETE FROM notifications", []))
        .await? as u64)
}
