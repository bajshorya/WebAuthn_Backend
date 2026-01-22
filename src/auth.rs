use crate::error::WebauthnError;
use crate::startup::AppState;
use axum::{
    extract::{Extension, Json, Path},
    http::StatusCode,
    response::IntoResponse,
};
use serde_json;
use tower_sessions::Session;
use tracing::{error, info};
use webauthn_rs::prelude::*;

pub async fn start_register(
    Extension(app_state): Extension<AppState>,
    session: Session,
    Path(username): Path<String>,
) -> Result<impl IntoResponse, WebauthnError> {
    info!("Start register");

    let user_unique_id = {
        let users_guard = app_state.users.lock().await;
        users_guard
            .name_to_id
            .get(&username)
            .copied()
            .unwrap_or_else(Uuid::new_v4)
    };

    let _ = session.remove_value("reg_state").await;

    let exclude_credentials = {
        let users_guard = app_state.users.lock().await;
        users_guard
            .keys
            .get(&user_unique_id)
            .map(|keys| keys.iter().map(|sk| sk.cred_id().clone()).collect())
    };

    let res = match app_state.webauthn.start_passkey_registration(
        user_unique_id,
        &username,
        &username,
        exclude_credentials,
    ) {
        Ok((ccr, reg_state)) => {
            session
                .insert("reg_state", (username.clone(), user_unique_id, reg_state))
                .await
                .map_err(|_| WebauthnError::Unknown)?;
            info!("Registration Started!");
            Json(ccr)
        }
        Err(e) => {
            error!("start_passkey_registration error: {:?}", e);
            return Err(WebauthnError::Unknown);
        }
    };
    Ok(res)
}

pub async fn finish_register(
    Extension(app_state): Extension<AppState>,
    session: Session,
    Json(reg): Json<RegisterPublicKeyCredential>,
) -> Result<impl IntoResponse, WebauthnError> {
    let (username, user_unique_id, reg_state): (String, Uuid, PasskeyRegistration) = session
        .get("reg_state")
        .await?
        .ok_or(WebauthnError::CorruptSession)?;

    let _ = session.remove_value("reg_state").await; 

    let res = match app_state
        .webauthn
        .finish_passkey_registration(&reg, &reg_state)
    {
        Ok(sk) => {
            let mut users_guard = app_state.users.lock().await;

            users_guard
                .keys
                .entry(user_unique_id)
                .and_modify(|keys| keys.push(sk.clone()))
                .or_insert_with(|| vec![sk.clone()]);

            users_guard.name_to_id.insert(username, user_unique_id);

            info!("Registration Successful!");

            (
                StatusCode::OK,
                axum::Json(serde_json::json!({
                    "status": "success",
                    "message": "Registration successful"
                })),
            )
        }
        Err(e) => {
            error!("finish_passkey_registration error: {:?}", e);
            (
                StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({
                    "status": "error",
                    "message": format!("Registration failed: {:?}", e)
                })),
            )
        }
    };
    Ok(res)
}
pub async fn start_authentication(
    Extension(app_state): Extension<AppState>,
    session: Session,
    Path(username): Path<String>,
) -> Result<impl IntoResponse, WebauthnError> {
    info!("Start Authentication");

    let _ = session.remove_value("auth_state").await;

    let users_guard = app_state.users.lock().await;

    let user_unique_id = users_guard
        .name_to_id
        .get(&username)
        .copied()
        .ok_or(WebauthnError::UserNotFound)?;

    let allow_credentials = users_guard
        .keys
        .get(&user_unique_id)
        .ok_or(WebauthnError::UserHasNoCredentials)?;

    let res = match app_state
        .webauthn
        .start_passkey_authentication(allow_credentials)
    {
        Ok((rcr, auth_state)) => {
            drop(users_guard);

            session
                .insert("auth_state", (user_unique_id, auth_state))
                .await
                .map_err(|_| WebauthnError::Unknown)?;
            Json(rcr)
        }
        Err(e) => {
            error!("start_passkey_authentication error: {:?}", e);
            return Err(WebauthnError::Unknown);
        }
    };
    Ok(res)
}

pub async fn finish_authentication(
    Extension(app_state): Extension<AppState>,
    session: Session,
    Json(auth): Json<PublicKeyCredential>,
) -> Result<impl IntoResponse, WebauthnError> {
    let (user_unique_id, auth_state): (Uuid, PasskeyAuthentication) = session
        .get("auth_state")
        .await?
        .ok_or(WebauthnError::CorruptSession)?;

    let _ = session.remove_value("auth_state").await;

    let res = match app_state
        .webauthn
        .finish_passkey_authentication(&auth, &auth_state)
    {
        Ok(auth_result) => {
            let mut users_guard = app_state.users.lock().await;

            users_guard
                .keys
                .get_mut(&user_unique_id)
                .map(|keys| {
                    keys.iter_mut().for_each(|sk| {
                        sk.update_credential(&auth_result);
                    })
                })
                .ok_or(WebauthnError::UserHasNoCredentials)?;

            info!("Authentication Successful!");

            (
                StatusCode::OK,
                axum::Json(serde_json::json!({
                    "status": "success",
                    "message": "Authentication successful"
                })),
            )
        }
        Err(e) => {
            error!("finish_passkey_authentication error: {:?}", e);
            (
                StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({
                    "status": "error",
                    "message": format!("Authentication failed: {:?}", e)
                })),
            )
        }
    };
    Ok(res)
}
