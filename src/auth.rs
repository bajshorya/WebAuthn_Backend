use crate::db;
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
use uuid::Uuid;
use webauthn_rs::prelude::*;

pub async fn start_register(
    Extension(app_state): Extension<AppState>,
    session: Session,
    Path(username): Path<String>,
) -> Result<impl IntoResponse, WebauthnError> {
    info!("Start register");

    let user_unique_id = match db::get_user_id(&app_state.db, &username).await {
        Ok(Some(id)) => id,
        Ok(None) => Uuid::new_v4(),
        Err(_) => return Err(WebauthnError::Unknown),
    };

    let _ = session.remove_value("reg_state").await;

    let exclude_credentials = match db::get_user_passkeys(&app_state.db, user_unique_id).await {
        Ok(keys) => Some(keys.iter().map(|sk| sk.cred_id().clone()).collect()),
        Err(_) => None,
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
            // Create user in database if it doesn't exist
            if let Err(e) = db::create_user(&app_state.db, user_unique_id, &username).await {
                // User might already exist, which is fine
                error!("Error creating user (may already exist): {:?}", e);
            }

            // Add the passkey to database
            if let Err(e) = db::add_passkey(&app_state.db, user_unique_id, &sk).await {
                error!("Error adding passkey to database: {:?}", e);
                return Err(WebauthnError::Unknown);
            }

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

    let user_unique_id = db::get_user_id(&app_state.db, &username)
        .await
        .map_err(|_| WebauthnError::Unknown)?
        .ok_or(WebauthnError::UserNotFound)?;

    let allow_credentials = db::get_user_passkeys(&app_state.db, user_unique_id)
        .await
        .map_err(|_| WebauthnError::Unknown)?;

    if allow_credentials.is_empty() {
        return Err(WebauthnError::UserHasNoCredentials);
    }

    let res = match app_state
        .webauthn
        .start_passkey_authentication(&allow_credentials)
    {
        Ok((rcr, auth_state)) => {
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
            // Get current passkeys from database
            let mut passkeys = db::get_user_passkeys(&app_state.db, user_unique_id)
                .await
                .map_err(|_| WebauthnError::Unknown)?;

            // Update all passkeys with new authentication data
            passkeys.iter_mut().for_each(|sk| {
                sk.update_credential(&auth_result);
            });

            // Save updated passkeys to database
            if let Err(e) = db::update_user_passkeys(&app_state.db, user_unique_id, &passkeys).await
            {
                error!("Error updating passkeys in database: {:?}", e);
                return Err(WebauthnError::Unknown);
            }

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
