use crate::db::connection::DbPool;
use sqlx::Error;
use uuid::Uuid;

pub async fn cast_vote(
    pool: &DbPool,
    poll_id: Uuid,
    option_id: Uuid,
    user_id: Uuid,
) -> Result<(), Error> {
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

pub async fn user_has_voted(pool: &DbPool, poll_id: Uuid, user_id: Uuid) -> Result<bool, Error> {
    let row = sqlx::query("SELECT id FROM votes WHERE poll_id = $1 AND user_id = $2")
        .bind(poll_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await?;

    Ok(row.is_some())
}
