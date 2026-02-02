use crate::db::connection::DbPool;
use sqlx::{Error, Row};
use uuid::Uuid;

pub async fn get_user_id(pool: &DbPool, username: &str) -> Result<Option<Uuid>, Error> {
    let row = sqlx::query("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_optional(pool)
        .await?;

    Ok(row.map(|r| r.get::<Uuid, _>("id")))
}

pub async fn create_user(pool: &DbPool, user_id: Uuid, username: &str) -> Result<(), Error> {
    sqlx::query("INSERT INTO users (id, username) VALUES ($1, $2)")
        .bind(user_id)
        .bind(username)
        .execute(pool)
        .await?;

    Ok(())
}
