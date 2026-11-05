use super::*;
use crate::db::{Database, DbError, now_millis};
use turso::{Row, params};

fn row_to_user(r: &Row) -> turso::Result<User> {
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
    db.connection().await.execute(
        "INSERT INTO users (id, display_name, email, avatar_path, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![user.id.clone(), user.display_name.clone(), user.email.clone(), user.avatar_path.clone(), user.created_at, user.updated_at],
    ).await?;
    Ok(user)
}

pub async fn get(db: &Database, id: &str) -> Result<Option<User>, DbError> {
    let conn = db.connection().await;
    let mut rows = conn
        .query(format!("SELECT {COLS} FROM users WHERE id = ?1"), [id])
        .await?;
    Ok(rows.next().await?.as_ref().map(row_to_user).transpose()?)
}

pub async fn list(db: &Database) -> Result<Vec<User>, DbError> {
    let conn = db.connection().await;
    let mut rows = conn
        .query(
            format!("SELECT {COLS} FROM users ORDER BY created_at, id"),
            (),
        )
        .await?;
    let mut users = Vec::new();
    while let Some(row) = rows.next().await? {
        users.push(row_to_user(&row)?);
    }
    Ok(users)
}

pub async fn update(db: &Database, user: &User) -> Result<User, DbError> {
    let now = now_millis();
    let n = db.connection().await.execute(
        "UPDATE users SET display_name = ?2, email = ?3, avatar_path = ?4, updated_at = ?5 WHERE id = ?1",
        params![user.id.clone(), user.display_name.clone(), user.email.clone(), user.avatar_path.clone(), now],
    ).await?;
    if n == 0 {
        return Err(DbError::NotFound);
    }
    Ok(User {
        updated_at: now,
        ..user.clone()
    })
}

pub async fn delete(db: &Database, id: &str) -> Result<bool, DbError> {
    Ok(db
        .connection()
        .await
        .execute("DELETE FROM users WHERE id = ?1", [id])
        .await?
        > 0)
}
