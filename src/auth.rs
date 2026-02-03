use crate::db;
use crate::error::WebauthnError;
use crate::startup::AppState;
use axum::{
    async_trait,
    extract::{Extension, FromRequestParts, Json, Path},
    http::{
        StatusCode,
        header::{AUTHORIZATION, HeaderMap},
        request::Parts,
    },
    response::IntoResponse,
};
use chrono::{Duration as ChronoDuration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use serde_json;
use tracing::{error, info};
use uuid::Uuid;
use webauthn_rs::prelude::*;

// JWT Claims
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,  // user_id
    pub exp: usize, // expiration time
    pub iat: usize, // issued at
    pub username: String,
}

// Authentication request/response types
#[derive(Debug, Deserialize)]
pub struct AuthRequest {
    pub username: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub user_id: Uuid,
    pub username: String,
}

// Bearer token extractor
#[derive(Debug)]
pub struct BearerAuth(pub Claims);

impl BearerAuth {
    pub async fn from_headers(
        headers: &HeaderMap,
        jwt_secret: &str,
    ) -> Result<Self, (StatusCode, String)> {
        let auth_header = headers
            .get(AUTHORIZATION)
            .ok_or((
                StatusCode::UNAUTHORIZED,
                "Missing Authorization header".to_string(),
            ))?
            .to_str()
            .map_err(|_| {
                (
                    StatusCode::UNAUTHORIZED,
                    "Invalid Authorization header".to_string(),
                )
            })?;

        if !auth_header.starts_with("Bearer ") {
            return Err((StatusCode::UNAUTHORIZED, "Invalid token format".to_string()));
        }

        let token = &auth_header[7..]; // Skip "Bearer "
        let claims = decode_jwt(token, jwt_secret)
            .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid token".to_string()))?;

        Ok(Self(claims))
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for BearerAuth
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, String);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Get AppState from extensions
        let app_state = parts.extensions.get::<AppState>().ok_or((
            StatusCode::INTERNAL_SERVER_ERROR,
            "AppState not found".to_string(),
        ))?;

        // Extract from headers
        Self::from_headers(&parts.headers, &app_state.jwt_secret).await
    }
}

// JWT helper functions
pub fn create_jwt(user_id: Uuid, username: &str, secret: &str) -> Result<String, WebauthnError> {
    let now = Utc::now();
    let expiration = now + ChronoDuration::days(7); // Token valid for 7 days

    let claims = Claims {
        sub: user_id,
        exp: expiration.timestamp() as usize,
        iat: now.timestamp() as usize,
        username: username.to_string(),
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|_| WebauthnError::TokenCreationError)
}

pub fn decode_jwt(token: &str, secret: &str) -> Result<Claims, WebauthnError> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|e| {
        error!("JWT decode error: {:?}", e);
        WebauthnError::InvalidToken
    })?;

    Ok(token_data.claims)
}

// Traditional username/password registration (optional - keeping for completeness)
pub async fn register_user(
    Extension(app_state): Extension<AppState>,
    Json(payload): Json<AuthRequest>,
) -> Result<impl IntoResponse, WebauthnError> {
    info!("Register user: {}", payload.username);

    let user_id = Uuid::new_v4();

    // Check if user already exists
    if let Ok(Some(_)) = db::get_user_id(&app_state.db, &payload.username).await {
        return Err(WebauthnError::UserAlreadyExists);
    }

    // Create user (without passkey)
    db::create_user(&app_state.db, user_id, &payload.username)
        .await
        .map_err(|_| WebauthnError::Unknown)?;

    // Create JWT token
    let token = create_jwt(user_id, &payload.username, &app_state.jwt_secret)?;

    let response = AuthResponse {
        access_token: token,
        token_type: "Bearer".to_string(),
        expires_in: 7 * 24 * 60 * 60, // 7 days in seconds
        user_id,
        username: payload.username,
    };

    Ok((StatusCode::CREATED, Json(response)))
}

// Traditional username/password authentication
pub async fn authenticate_user(
    Extension(app_state): Extension<AppState>,
    Json(payload): Json<AuthRequest>,
) -> Result<impl IntoResponse, WebauthnError> {
    info!("Authenticate user: {}", payload.username);

    let user_id = db::get_user_id(&app_state.db, &payload.username)
        .await
        .map_err(|_| WebauthnError::Unknown)?
        .ok_or(WebauthnError::UserNotFound)?;

    // Create JWT token
    let token = create_jwt(user_id, &payload.username, &app_state.jwt_secret)?;

    let response = AuthResponse {
        access_token: token,
        token_type: "Bearer".to_string(),
        expires_in: 7 * 24 * 60 * 60, // 7 days in seconds
        user_id,
        username: payload.username,
    };

    Ok((StatusCode::OK, Json(response)))
}

// WebAuthn registration endpoints
pub async fn start_register(
    Extension(app_state): Extension<AppState>,
    Path(username): Path<String>,
) -> Result<impl IntoResponse, WebauthnError> {
    info!("Start WebAuthn register for: {}", username);

    let user_unique_id = match db::get_user_id(&app_state.db, &username).await {
        Ok(Some(id)) => id,
        Ok(None) => Uuid::new_v4(),
        Err(_) => return Err(WebauthnError::Unknown),
    };

    let exclude_credentials = match db::get_user_passkeys(&app_state.db, user_unique_id).await {
        Ok(keys) => Some(
            keys.iter()
                .map(|sk: &Passkey| sk.cred_id().clone())
                .collect(),
        ),
        Err(_) => None,
    };

    let (ccr, reg_state) = app_state
        .webauthn
        .start_passkey_registration(user_unique_id, &username, &username, exclude_credentials)
        .map_err(|e| {
            error!("start_passkey_registration error: {:?}", e);
            WebauthnError::Unknown
        })?;

    info!("WebAuthn registration started for: {}", username);

    // In a real app, you'd want to store this server-side with an expiration
    let state_response = serde_json::json!({
        "public_key": ccr,
        "registration_state": serde_json::to_value(&reg_state).map_err(|_| WebauthnError::Unknown)?,
        "user_id": user_unique_id,
        "username": username
    });

    Ok(Json(state_response))
}

pub async fn finish_register(
    Extension(app_state): Extension<AppState>,
    Json(payload): Json<FinishRegisterRequest>,
) -> Result<impl IntoResponse, WebauthnError> {
    info!("Finish WebAuthn register for user_id: {}", payload.user_id);

    let reg_state: PasskeyRegistration = serde_json::from_value(payload.registration_state)
        .map_err(|e| {
            error!("Failed to deserialize registration state: {:?}", e);
            WebauthnError::Unknown
        })?;

    let res = match app_state
        .webauthn
        .finish_passkey_registration(&payload.credential, &reg_state)
    {
        Ok(sk) => {
            // Create user if they don't exist
            if let Err(e) = db::create_user(&app_state.db, payload.user_id, &payload.username).await
            {
                error!("Error creating user (may already exist): {:?}", e);
            }

            // Add passkey
            if let Err(e) = db::add_passkey(&app_state.db, payload.user_id, &sk).await {
                error!("Error adding passkey to database: {:?}", e);
                return Err(WebauthnError::Unknown);
            }

            // Create JWT token
            let token = create_jwt(payload.user_id, &payload.username, &app_state.jwt_secret)?;

            info!("WebAuthn registration successful for: {}", payload.username);

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "success",
                    "message": "Registration successful",
                    "access_token": token,
                    "token_type": "Bearer",
                    "expires_in": 7 * 24 * 60 * 60,
                    "user_id": payload.user_id,
                    "username": payload.username
                })),
            )
        }
        Err(e) => {
            error!("finish_passkey_registration error: {:?}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "status": "error",
                    "message": format!("Registration failed: {:?}", e)
                })),
            )
        }
    };
    Ok(res)
}

// WebAuthn authentication endpoints
pub async fn start_authentication(
    Extension(app_state): Extension<AppState>,
    Path(username): Path<String>,
) -> Result<impl IntoResponse, WebauthnError> {
    info!("Start WebAuthn authentication for: {}", username);

    let user_unique_id = db::get_user_id(&app_state.db, &username)
        .await
        .map_err(|_| WebauthnError::Unknown)?
        .ok_or(WebauthnError::UserNotFound)?;

    let allow_credentials: Vec<Passkey> = db::get_user_passkeys(&app_state.db, user_unique_id)
        .await
        .map_err(|_| WebauthnError::Unknown)?;

    if allow_credentials.is_empty() {
        return Err(WebauthnError::UserHasNoCredentials);
    }

    let (rcr, auth_state) = app_state
        .webauthn
        .start_passkey_authentication(&allow_credentials)
        .map_err(|e| {
            error!("start_passkey_authentication error: {:?}", e);
            WebauthnError::Unknown
        })?;

    info!("WebAuthn authentication started for: {}", username);

    let state_response = serde_json::json!({
        "public_key": rcr,
        "authentication_state": serde_json::to_value(&auth_state).map_err(|_| WebauthnError::Unknown)?,
        "user_id": user_unique_id,
        "username": username
    });

    Ok(Json(state_response))
}

pub async fn finish_authentication(
    Extension(app_state): Extension<AppState>,
    Json(payload): Json<FinishAuthRequest>,
) -> Result<impl IntoResponse, WebauthnError> {
    info!(
        "Finish WebAuthn authentication for user_id: {}",
        payload.user_id
    );

    let auth_state: PasskeyAuthentication = serde_json::from_value(payload.authentication_state)
        .map_err(|e| {
            error!("Failed to deserialize authentication state: {:?}", e);
            WebauthnError::Unknown
        })?;

    let res = match app_state
        .webauthn
        .finish_passkey_authentication(&payload.credential, &auth_state)
    {
        Ok(auth_result) => {
            let mut passkeys = db::get_user_passkeys(&app_state.db, payload.user_id)
                .await
                .map_err(|_| WebauthnError::Unknown)?;

            passkeys.iter_mut().for_each(|sk: &mut Passkey| {
                sk.update_credential(&auth_result);
            });

            if let Err(e) =
                db::update_user_passkeys(&app_state.db, payload.user_id, &passkeys).await
            {
                error!("Error updating passkeys in database: {:?}", e);
                return Err(WebauthnError::Unknown);
            }

            // Create JWT token
            let token = create_jwt(payload.user_id, &payload.username, &app_state.jwt_secret)?;

            info!(
                "WebAuthn authentication successful for: {}",
                payload.username
            );

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "success",
                    "message": "Authentication successful",
                    "access_token": token,
                    "token_type": "Bearer",
                    "expires_in": 7 * 24 * 60 * 60,
                    "user_id": payload.user_id,
                    "username": payload.username
                })),
            )
        }
        Err(e) => {
            error!("finish_passkey_authentication error: {:?}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "status": "error",
                    "message": format!("Authentication failed: {:?}", e)
                })),
            )
        }
    };
    Ok(res)
}

// Request types for WebAuthn flows
#[derive(Debug, Deserialize)]
pub struct FinishRegisterRequest {
    pub credential: RegisterPublicKeyCredential,
    pub registration_state: serde_json::Value,
    pub user_id: Uuid,
    pub username: String,
}

#[derive(Debug, Deserialize)]
pub struct FinishAuthRequest {
    pub credential: PublicKeyCredential,
    pub authentication_state: serde_json::Value,
    pub user_id: Uuid,
    pub username: String,
}
