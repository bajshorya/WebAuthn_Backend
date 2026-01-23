use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres, Row};
use uuid::Uuid;
use webauthn_rs::prelude::Passkey;

pub type DbPool = Pool<Postgres>;

pub async fn init_db(database_url: &str) -> Result<DbPool, sqlx::Error> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;

    // Create users table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            id UUID PRIMARY KEY,
            username VARCHAR(255) NOT NULL UNIQUE,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(&pool)
    .await?;

    // Create passkeys table to store serialized passkey data
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS passkeys (
            id SERIAL PRIMARY KEY,
            user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            passkey_data JSON NOT NULL,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(&pool)
    .await?;

    // Create username to id mapping table for quick lookups
    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_users_username ON users(username)
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_passkeys_user_id ON passkeys(user_id)
        "#,
    )
    .execute(&pool)
    .await?;

    Ok(pool)
}

pub async fn get_user_id(pool: &DbPool, username: &str) -> Result<Option<Uuid>, sqlx::Error> {
    let row = sqlx::query("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_optional(pool)
        .await?;

    Ok(row.map(|r| r.get::<Uuid, _>("id")))
}

pub async fn create_user(pool: &DbPool, user_id: Uuid, username: &str) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO users (id, username) VALUES ($1, $2)")
        .bind(user_id)
        .bind(username)
        .execute(pool)
        .await?;

    Ok(())
}


pub async fn add_passkey(
    pool: &DbPool,
    user_id: Uuid,
    passkey: &Passkey,
) -> Result<(), sqlx::Error> {
    let passkey_json = serde_json::to_value(passkey).unwrap_or(serde_json::Value::Null);

    sqlx::query("INSERT INTO passkeys (user_id, passkey_data) VALUES ($1, $2)")
        .bind(user_id)
        .bind(passkey_json) // Now passing as serde_json::Value instead of String
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn get_user_passkeys(pool: &DbPool, user_id: Uuid) -> Result<Vec<Passkey>, sqlx::Error> {
    use sqlx::types::Json;

    let rows = sqlx::query("SELECT passkey_data FROM passkeys WHERE user_id = $1")
        .bind(user_id)
        .fetch_all(pool)
        .await?;

    let passkeys: Vec<Passkey> = rows
        .into_iter()
        .filter_map(|row| {
            let json_val: Json<Passkey> = row.get("passkey_data");
            Some(json_val.0)
        })
        .collect();

    Ok(passkeys)
}

/// Update all passkeys for a user (used after authentication to update counters)
pub async fn update_user_passkeys(
    pool: &DbPool,
    user_id: Uuid,
    passkeys: &[Passkey],
) -> Result<(), sqlx::Error> {
    // Delete old passkeys
    sqlx::query("DELETE FROM passkeys WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await?;

    // Insert updated passkeys
    for passkey in passkeys {
        add_passkey(pool, user_id, passkey).await?;
    }

    Ok(())
}
