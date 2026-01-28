use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::types::chrono::{DateTime, Utc};
use sqlx::{Pool, Postgres, Row};
use uuid::Uuid;
use webauthn_rs::prelude::Passkey;

pub type DbPool = Pool<Postgres>;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Poll {
    pub id: Uuid,
    pub creator_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    #[sqlx(try_from = "DateTime<Utc>")]
    pub created_at: DateTime<Utc>,
    pub closed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollOption {
    pub id: Uuid,
    pub poll_id: Uuid,
    pub option_text: String,
    pub votes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    pub id: Uuid,
    pub poll_id: Uuid,
    pub option_id: Uuid,
    pub user_id: Uuid,
    pub created_at: DateTime<Utc>,
}

pub async fn init_db(database_url: &str) -> Result<DbPool, sqlx::Error> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;

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

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS polls (
            id UUID PRIMARY KEY,
            creator_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            title VARCHAR(255) NOT NULL,
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
            closed BOOLEAN NOT NULL DEFAULT FALSE
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS poll_options (
            id UUID PRIMARY KEY,
            poll_id UUID NOT NULL REFERENCES polls(id) ON DELETE CASCADE,
            option_text VARCHAR(255) NOT NULL,
            votes INT NOT NULL DEFAULT 0
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS votes (
            id UUID PRIMARY KEY,
            poll_id UUID NOT NULL REFERENCES polls(id) ON DELETE CASCADE,
            option_id UUID NOT NULL REFERENCES poll_options(id) ON DELETE CASCADE,
            user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(poll_id, user_id)
        )
        "#,
    )
    .execute(&pool)
    .await?;

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

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_polls_creator_id ON polls(creator_id)
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_poll_options_poll_id ON poll_options(poll_id)
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_votes_poll_id ON votes(poll_id)
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_votes_user_id ON votes(user_id)
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
        .bind(passkey_json)
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

pub async fn update_user_passkeys(
    pool: &DbPool,
    user_id: Uuid,
    passkeys: &[Passkey],
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM passkeys WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await?;

    for passkey in passkeys {
        add_passkey(pool, user_id, passkey).await?;
    }

    Ok(())
}

pub async fn create_poll(
    pool: &DbPool,
    creator_id: Uuid,
    title: &str,
    description: Option<&str>,
) -> Result<Uuid, sqlx::Error> {
    let poll_id = Uuid::new_v4();

    sqlx::query("INSERT INTO polls (id, creator_id, title, description) VALUES ($1, $2, $3, $4)")
        .bind(poll_id)
        .bind(creator_id)
        .bind(title)
        .bind(description)
        .execute(pool)
        .await?;

    Ok(poll_id)
}

pub async fn add_poll_option(
    pool: &DbPool,
    poll_id: Uuid,
    option_text: &str,
) -> Result<Uuid, sqlx::Error> {
    let option_id = Uuid::new_v4();

    sqlx::query("INSERT INTO poll_options (id, poll_id, option_text) VALUES ($1, $2, $3)")
        .bind(option_id)
        .bind(poll_id)
        .bind(option_text)
        .execute(pool)
        .await?;

    Ok(option_id)
}

pub async fn get_poll(pool: &DbPool, poll_id: Uuid) -> Result<Option<Poll>, sqlx::Error> {
    let row = sqlx::query_as::<_, Poll>(
        "SELECT id, creator_id, title, description, created_at, closed FROM polls WHERE id = $1",
    )
    .bind(poll_id)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

pub async fn get_all_polls(pool: &DbPool) -> Result<Vec<Poll>, sqlx::Error> {
    let rows = sqlx::query_as::<_, Poll>(
        "SELECT id, creator_id, title, description, created_at, closed FROM polls ORDER BY created_at DESC"
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

pub async fn get_poll_options(
    pool: &DbPool,
    poll_id: Uuid,
) -> Result<Vec<PollOption>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, poll_id, option_text, votes FROM poll_options WHERE poll_id = $1 ORDER BY option_text"
    )
    .bind(poll_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| PollOption {
            id: r.get("id"),
            poll_id: r.get("poll_id"),
            option_text: r.get("option_text"),
            votes: r.get("votes"),
        })
        .collect())
}

pub async fn cast_vote(
    pool: &DbPool,
    poll_id: Uuid,
    option_id: Uuid,
    user_id: Uuid,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    let existing_vote = sqlx::query("SELECT id FROM votes WHERE poll_id = $1 AND user_id = $2")
        .bind(poll_id)
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await?;

    if existing_vote.is_some() {
        tx.rollback().await?;
        return Err(sqlx::Error::RowNotFound);
    }

    let vote_id = Uuid::new_v4();
    sqlx::query("INSERT INTO votes (id, poll_id, option_id, user_id) VALUES ($1, $2, $3, $4)")
        .bind(vote_id)
        .bind(poll_id)
        .bind(option_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("UPDATE poll_options SET votes = votes + 1 WHERE id = $1")
        .bind(option_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

pub async fn user_has_voted(
    pool: &DbPool,
    poll_id: Uuid,
    user_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let row = sqlx::query("SELECT id FROM votes WHERE poll_id = $1 AND user_id = $2")
        .bind(poll_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await?;

    Ok(row.is_some())
}

pub async fn close_poll(pool: &DbPool, poll_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE polls SET closed = TRUE WHERE id = $1")
        .bind(poll_id)
        .execute(pool)
        .await?;

    Ok(())
}
