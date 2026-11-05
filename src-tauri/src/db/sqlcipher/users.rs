use super::*;
use crate::db::{Database, DbError, now_millis};
use rusqlite::{OptionalExtension, Row, params};

fn row_to_user(r: &Row) -> rusqlite::Result<User> {
    Ok(User {
        id: r.get(0)?,
        display_name: r.get(1)?,
        email: r.get(2)?,
        avatar_path: r.get(3)?,
        created_at: r.get(4)?,
        updated_at: r.get(5)?,
    })
}

const COLS: &str = "id, display_name, email, avatar_path, created_at, updated_at";

pub async fn create(db: &Database, new: NewUser) -> Result<User, DbError> {
    let now = now_millis();
    let user = User {
        id: uuid::Uuid::new_v4().to_string(),
        display_name: new.display_name,
        email: new.email,
        avatar_path: new.avatar_path,
        created_at: now,
        updated_at: now,
    };
    let stored = user.clone();
    db.with(move |c| {
        c.execute(
            "INSERT INTO users (id, display_name, email, avatar_path, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                stored.id,
                stored.display_name,
                stored.email,
                stored.avatar_path,
                stored.created_at,
                stored.updated_at
            ],
        )
    }).await?;
    Ok(user)
}

pub async fn get(db: &Database, id: &str) -> Result<Option<User>, DbError> {
    let id = id.to_owned();
    db.with(move |c| {
        c.query_row(
            &format!("SELECT {COLS} FROM users WHERE id = ?1"),
            [id],
            row_to_user,
        )
        .optional()
    })
    .await
}

pub async fn list(db: &Database) -> Result<Vec<User>, DbError> {
    db.with(move |c| {
        let mut st = c.prepare(&format!("SELECT {COLS} FROM users ORDER BY created_at, id"))?;
        st.query_map([], row_to_user)?.collect()
    })
    .await
}

/// Full-row update; `updated_at` is set here. Returns the stored row.
pub async fn update(db: &Database, user: &User) -> Result<User, DbError> {
    let now = now_millis();
    let stored = user.clone();
    let n = db.with(move |c| {
        c.execute(
            "UPDATE users SET display_name = ?2, email = ?3, avatar_path = ?4, updated_at = ?5 WHERE id = ?1",
            params![stored.id, stored.display_name, stored.email, stored.avatar_path, now],
        )
    }).await?;
    if n == 0 {
        return Err(DbError::NotFound);
    }
    Ok(User {
        updated_at: now,
        ..user.clone()
    })
}

pub async fn delete(db: &Database, id: &str) -> Result<bool, DbError> {
    let id = id.to_owned();
    Ok(db
        .with(move |c| c.execute("DELETE FROM users WHERE id = ?1", [id]))
        .await?
        > 0)
}
