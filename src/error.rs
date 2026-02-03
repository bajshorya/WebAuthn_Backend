use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum WebauthnError {
    #[error("unknown webauthn error")]
    Unknown,
    #[error("Corrupt Session")]
    CorruptSession,
    #[error("User Not Found")]
    UserNotFound,
    #[error("User Has No Credentials")]
    UserHasNoCredentials,
    #[error("Unauthorized")]
    Unauthorized,
    #[error("Invalid token")]
    InvalidToken,
    #[error("Token creation error")]
    TokenCreationError,
    #[error("User already exists")]
    UserAlreadyExists,
}

#[derive(Error, Debug)]
pub enum PollError {
    #[error("Unauthorized")]
    Unauthorized,
    #[error("Invalid request")]
    InvalidRequest,
    #[error("Poll not found")]
    PollNotFound,
    #[error("Poll option not found")]
    OptionNotFound,
    #[error("Poll is closed")]
    PollClosed,
    #[error("User already voted on this poll")]
    AlreadyVoted,
    #[error("Database error: {0}")]
    DatabaseError(String),
}

impl IntoResponse for WebauthnError {
    fn into_response(self) -> Response {
        let (status, error_message) = match &self {
            WebauthnError::Unknown => (StatusCode::INTERNAL_SERVER_ERROR, "Unknown error"),
            WebauthnError::CorruptSession => (StatusCode::BAD_REQUEST, "Corrupt session"),
            WebauthnError::UserNotFound => (StatusCode::NOT_FOUND, "User not found"),
            WebauthnError::UserHasNoCredentials => (
                StatusCode::BAD_REQUEST,
                "User has no registered credentials",
            ),
            WebauthnError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized"),
            WebauthnError::InvalidToken => (StatusCode::UNAUTHORIZED, "Invalid token"),
            WebauthnError::TokenCreationError => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create token")
            }
            WebauthnError::UserAlreadyExists => (StatusCode::CONFLICT, "User already exists"),
        };

        let body = Json(json!({
            "error": error_message,
            "details": self.to_string()
        }));

        (status, body).into_response()
    }
}

impl IntoResponse for PollError {
    fn into_response(self) -> Response {
        let (status, error_message) = match &self {
            PollError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized"),
            PollError::InvalidRequest => (StatusCode::BAD_REQUEST, "Invalid request"),
            PollError::PollNotFound => (StatusCode::NOT_FOUND, "Poll not found"),
            PollError::OptionNotFound => (StatusCode::NOT_FOUND, "Poll option not found"),
            PollError::PollClosed => (StatusCode::BAD_REQUEST, "Poll is closed"),
            PollError::AlreadyVoted => (StatusCode::CONFLICT, "User already voted on this poll"),
            PollError::DatabaseError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.as_str()),
        };

        let body = Json(json!({
            "error": error_message,
            "details": self.to_string()
        }));

        (status, body).into_response()
    }
}

impl From<sqlx::Error> for PollError {
    fn from(error: sqlx::Error) -> Self {
        PollError::DatabaseError(error.to_string())
    }
}

impl From<jsonwebtoken::errors::Error> for WebauthnError {
    fn from(_: jsonwebtoken::errors::Error) -> Self {
        WebauthnError::InvalidToken
    }
}

impl From<serde_json::Error> for WebauthnError {
    fn from(_: serde_json::Error) -> Self {
        WebauthnError::Unknown
    }
}
