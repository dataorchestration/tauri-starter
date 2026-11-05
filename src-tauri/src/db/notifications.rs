//! In-app notification center backed by the encrypted store.

use rusqlite::{Row, params};
use serde::{Deserialize, Serialize};

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

pub fn insert(
    db: &Database,
    title: &str,
    body: &str,
    kind: NotificationKind,
) -> Result<Notification, DbError> {
    let now = now_millis();
    let id = db.with(|c| {
        c.execute(
            "INSERT INTO notifications (title, body, kind, read, created_at) VALUES (?1, ?2, ?3, 0, ?4)",
            params![title, body, kind.as_str(), now],
        )?;
        Ok(c.last_insert_rowid())
    })?;
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
pub fn list(db: &Database, limit: u32) -> Result<Vec<Notification>, DbError> {
    db.with(|c| {
        let mut st = c.prepare(&format!(
            "SELECT {COLS} FROM notifications ORDER BY created_at DESC, id DESC LIMIT ?1"
        ))?;
        st.query_map([limit], row_to_notification)?.collect()
    })
}

pub fn unread_count(db: &Database) -> Result<i64, DbError> {
    db.with(|c| {
        c.query_row(
            "SELECT count(*) FROM notifications WHERE read = 0",
            [],
            |r| r.get(0),
        )
    })
}

pub fn mark_read(db: &Database, id: i64, read: bool) -> Result<bool, DbError> {
    Ok(db.with(|c| {
        c.execute(
            "UPDATE notifications SET read = ?2 WHERE id = ?1",
            params![id, read as i64],
        )
    })? > 0)
}

pub fn mark_all_read(db: &Database) -> Result<usize, DbError> {
    db.with(|c| c.execute("UPDATE notifications SET read = 1 WHERE read = 0", []))
}

pub fn delete(db: &Database, id: i64) -> Result<bool, DbError> {
    Ok(db.with(|c| c.execute("DELETE FROM notifications WHERE id = ?1", [id]))? > 0)
}

pub fn clear(db: &Database) -> Result<usize, DbError> {
    db.with(|c| c.execute("DELETE FROM notifications", []))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_list_read_delete() {
        let db = Database::open_in_memory().unwrap();
        let a = insert(&db, "A", "first", NotificationKind::Info).unwrap();
        let b = insert(&db, "B", "second", NotificationKind::Error).unwrap();
        let all = list(&db, 10).unwrap();
        assert_eq!(
            all.iter().map(|n| n.id).collect::<Vec<_>>(),
            vec![b.id, a.id],
            "newest first"
        );
        assert_eq!(all[0].kind, NotificationKind::Error);
        assert_eq!(unread_count(&db).unwrap(), 2);

        assert!(mark_read(&db, a.id, true).unwrap());
        assert_eq!(unread_count(&db).unwrap(), 1);
        assert_eq!(mark_all_read(&db).unwrap(), 1);
        assert_eq!(unread_count(&db).unwrap(), 0);

        assert!(delete(&db, a.id).unwrap());
        assert_eq!(list(&db, 10).unwrap().len(), 1);
        assert_eq!(clear(&db).unwrap(), 1);
        assert!(list(&db, 10).unwrap().is_empty());
    }

    #[test]
    fn kind_wire_format_is_snake_case() {
        assert_eq!(
            serde_json::to_string(&NotificationKind::Warning).unwrap(),
            "\"warning\""
        );
        assert_eq!(NotificationKind::parse("bogus"), NotificationKind::Info);
    }
}
