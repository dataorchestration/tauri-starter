//! In-app notification center backed by the encrypted store.

use serde::{Deserialize, Serialize};

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
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
    pub(crate) fn parse(s: &str) -> Self {
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

#[cfg(feature = "database-turso")]
#[path = "turso/notifications.rs"]
mod backend;
#[cfg(feature = "database-sqlcipher")]
#[path = "sqlcipher/notifications.rs"]
mod backend;
pub use backend::{clear, delete, insert, list, mark_all_read, mark_read, unread_count};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

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
