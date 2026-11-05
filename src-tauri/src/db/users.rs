//! Local user profiles. This is a profile table, not authentication: there is
//! no password here. Wire it to your auth provider by storing the remote id in
//! `id` (or add a column in a new migration).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub display_name: String,
    pub email: Option<String>,
    pub avatar_path: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NewUser {
    pub display_name: String,
    pub email: Option<String>,
    pub avatar_path: Option<String>,
}

#[cfg(feature = "database-turso")]
#[path = "turso/users.rs"]
mod backend;
#[cfg(feature = "database-sqlcipher")]
#[path = "sqlcipher/users.rs"]
mod backend;
pub use backend::{create, delete, get, list, update};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Database, DbError};

    #[tokio::test]
    async fn crud_roundtrip() {
        let db = Database::open_in_memory().await.unwrap();
        let u = create(
            &db,
            NewUser {
                display_name: "Ada".into(),
                email: Some("ada@example.com".into()),
                avatar_path: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(get(&db, &u.id).await.unwrap().unwrap(), u);
        assert_eq!(list(&db).await.unwrap().len(), 1);

        let renamed = User {
            display_name: "Ada L.".into(),
            ..u.clone()
        };
        let stored = update(&db, &renamed).await.unwrap();
        assert_eq!(stored.display_name, "Ada L.");
        assert!(stored.updated_at >= u.updated_at);

        assert!(delete(&db, &u.id).await.unwrap());
        assert!(!delete(&db, &u.id).await.unwrap());
        assert!(get(&db, &u.id).await.unwrap().is_none());
        assert!(matches!(
            update(&db, &renamed).await,
            Err(DbError::NotFound)
        ));
    }
}
