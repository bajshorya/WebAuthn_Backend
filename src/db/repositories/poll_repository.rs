use crate::db::connection::DbPool;
use crate::db::models::{Poll, PollOption};
use sqlx::Error;
use sqlx::Row;
use uuid::Uuid;

pub async fn create_poll(
    pool: &DbPool,
    creator_id: Uuid,
    title: &str,
    description: Option<&str>,
) -> Result<Uuid, Error> {
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
) -> Result<Uuid, Error> {
    let option_id = Uuid::new_v4();

    sqlx::query("INSERT INTO poll_options (id, poll_id, option_text) VALUES ($1, $2, $3)")
        .bind(option_id)
        .bind(poll_id)
        .bind(option_text)
        .execute(pool)
        .await?;

    Ok(option_id)
}

pub async fn get_poll(pool: &DbPool, poll_id: Uuid) -> Result<Option<Poll>, Error> {
    let row = sqlx::query_as::<_, Poll>(
        "SELECT id, creator_id, title, description, created_at, closed FROM polls WHERE id = $1",
    )
    .bind(poll_id)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

pub async fn get_all_polls(pool: &DbPool) -> Result<Vec<Poll>, Error> {
    let rows = sqlx::query_as::<_, Poll>(
        "SELECT id, creator_id, title, description, created_at, closed FROM polls ORDER BY created_at DESC"
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

pub async fn get_poll_options(pool: &DbPool, poll_id: Uuid) -> Result<Vec<PollOption>, Error> {
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

pub async fn close_poll(pool: &DbPool, poll_id: Uuid) -> Result<(), Error> {
    sqlx::query("UPDATE polls SET closed = TRUE WHERE id = $1")
        .bind(poll_id)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn restart_poll(pool: &DbPool, poll_id: Uuid) -> Result<(), Error> {
    sqlx::query("UPDATE polls SET closed = FALSE WHERE id = $1")
        .bind(poll_id)
        .execute(pool)
        .await?;

    Ok(())
}
