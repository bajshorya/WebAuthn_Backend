use crate::db::connection::DbPool;
use sqlx::Error;
use sqlx::Row;
use sqlx::types::Json;
use uuid::Uuid;
use webauthn_rs::prelude::Passkey;

pub async fn add_passkey(pool: &DbPool, user_id: Uuid, passkey: &Passkey) -> Result<(), Error> {
    let passkey_json = serde_json::to_value(passkey).unwrap_or(serde_json::Value::Null);

    sqlx::query("INSERT INTO passkeys (user_id, passkey_data) VALUES ($1, $2)")
        .bind(user_id)
        .bind(passkey_json)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn get_user_passkeys(pool: &DbPool, user_id: Uuid) -> Result<Vec<Passkey>, Error> {
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
) -> Result<(), Error> {
    sqlx::query("DELETE FROM passkeys WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await?;

    for passkey in passkeys {
        add_passkey(pool, user_id, passkey).await?;
    }

    Ok(())
}
