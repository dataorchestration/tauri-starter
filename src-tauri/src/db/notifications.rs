//! In-app notification center backed by the encrypted store.

use serde::{Deserialize, Serialize};
use turso::{Row, params};

use super::{Database, DbError, now_millis};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NotificationKind {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

impl NotificationKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
    fn parse(s: &str) -> Self {
        match s {
            "success" => Self::Success,
            "warning" => Self::Warning,
            "error" => Self::Error,
            _ => Self::Info,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notification {
    pub id: i64,
    pub title: String,
    pub body: String,
    pub kind: NotificationKind,
    pub read: bool,
    pub created_at: i64,
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn insert_list_read_delete() {
        let db = Database::open_in_memory().await.unwrap();
        let a = insert(&db, "A", "first", NotificationKind::Info)
            .await
            .unwrap();
        let b = insert(&db, "B", "second", NotificationKind::Error)
            .await
            .unwrap();
        let all = list(&db, 10).await.unwrap();
        assert_eq!(
            all.iter().map(|n| n.id).collect::<Vec<_>>(),
            vec![b.id, a.id],
            "newest first"
        );
        assert_eq!(all[0].kind, NotificationKind::Error);
        assert_eq!(unread_count(&db).await.unwrap(), 2);

        assert!(mark_read(&db, a.id, true).await.unwrap());
        assert_eq!(unread_count(&db).await.unwrap(), 1);
        assert_eq!(mark_all_read(&db).await.unwrap(), 1);
        assert_eq!(unread_count(&db).await.unwrap(), 0);

        assert!(delete(&db, a.id).await.unwrap());
        assert_eq!(list(&db, 10).await.unwrap().len(), 1);
        assert_eq!(clear(&db).await.unwrap(), 1);
        assert!(list(&db, 10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn kind_wire_format_is_snake_case() {
        assert_eq!(
            serde_json::to_string(&NotificationKind::Warning).unwrap(),
            "\"warning\""
        );
        assert_eq!(NotificationKind::parse("bogus"), NotificationKind::Info);
    }
}
