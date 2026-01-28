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
    #[error("Deserialising Session failed: {0}")]
    InvalidSessionState(#[from] tower_sessions::session::Error),
}

#[derive(Error, Debug)]
pub enum PollError {
    #[error("Unauthorized")]
    Unauthorized,
    #[error("Poll not found")]
    PollNotFound,
    #[error("Poll option not found")]
    OptionNotFound,
    #[error("User already voted on this poll")]
    AlreadyVoted,
    #[error("Poll is closed")]
    PollClosed,
    #[error("Database error: {0}")]
    DatabaseError(String),
    #[error("Invalid request")]
    InvalidRequest,
}

impl IntoResponse for WebauthnError {
    fn into_response(self) -> Response {
        let body = match self {
            WebauthnError::CorruptSession => "Corrupt Session",
            WebauthnError::UserNotFound => "User Not Found",
            WebauthnError::Unknown => "Unknown Error",
            WebauthnError::UserHasNoCredentials => "User Has No Credentials",
            WebauthnError::InvalidSessionState(_) => "Deserialising Session failed",
        };

        (StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
    }
}

impl IntoResponse for PollError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            PollError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized"),
            PollError::PollNotFound => (StatusCode::NOT_FOUND, "Poll not found"),
            PollError::OptionNotFound => (StatusCode::NOT_FOUND, "Poll option not found"),
            PollError::AlreadyVoted => (StatusCode::CONFLICT, "User already voted on this poll"),
            PollError::PollClosed => (StatusCode::FORBIDDEN, "Poll is closed"),
            PollError::DatabaseError(ref msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.as_str()),
            PollError::InvalidRequest => (StatusCode::BAD_REQUEST, "Invalid request"),
        };

        let body = Json(json!({
            "error": error_message
        }));

        (status, body).into_response()
    }
}
