use crate::db;
use crate::error::PollError;
use crate::startup::AppState;
use axum::{
    extract::{Extension, Json, Path},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower_sessions::Session;
use uuid::Uuid;

// Request/Response DTOs
#[derive(Debug, Deserialize)]
pub struct CreatePollRequest {
    pub title: String,
    pub description: Option<String>,
    pub options: Vec<String>, // List of poll options
}

#[derive(Debug, Serialize)]
pub struct CreatePollResponse {
    pub poll_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub options: Vec<PollOptionResponse>,
}

#[derive(Debug, Serialize)]
pub struct PollOptionResponse {
    pub id: Uuid,
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct PollResponse {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub creator_id: Uuid,
    pub created_at: String,
    pub closed: bool,
    pub options: Vec<PollOptionWithVotesResponse>,
    pub user_voted: bool,
}

#[derive(Debug, Serialize)]
pub struct PollOptionWithVotesResponse {
    pub id: Uuid,
    pub text: String,
    pub votes: i64,
}

#[derive(Debug, Deserialize)]
pub struct CastVoteRequest {
    pub option_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct VoteResponse {
    pub success: bool,
    pub message: String,
}

// Helper function to extract user_id from session
async fn get_user_id_from_session(session: &Session) -> Result<Uuid, PollError> {
    session
        .get::<Uuid>("user_id")
        .await
        .map_err(|_| PollError::Unauthorized)?
        .ok_or(PollError::Unauthorized)
}

/// Create a new poll (authenticated users only)
pub async fn create_poll(
    Extension(app_state): Extension<AppState>,
    session: Session,
    Json(payload): Json<CreatePollRequest>,
) -> Result<impl IntoResponse, PollError> {
    // Verify user is authenticated
    let user_id = get_user_id_from_session(&session).await?;

    // Validate input
    if payload.title.is_empty() || payload.options.is_empty() {
        return Err(PollError::InvalidRequest);
    }

    if payload.options.len() < 2 {
        return Err(PollError::InvalidRequest);
    }

    // Create poll in database
    let poll_id = db::create_poll(
        &app_state.db,
        user_id,
        &payload.title,
        payload.description.as_deref(),
    )
    .await
    .map_err(|e| PollError::DatabaseError(e.to_string()))?;

    // Add poll options
    let mut option_responses = Vec::new();
    for option_text in payload.options {
        let option_id = db::add_poll_option(&app_state.db, poll_id, &option_text)
            .await
            .map_err(|e| PollError::DatabaseError(e.to_string()))?;

        option_responses.push(PollOptionResponse {
            id: option_id,
            text: option_text,
        });
    }

    let response = CreatePollResponse {
        poll_id,
        title: payload.title,
        description: payload.description,
        options: option_responses,
    };

    Ok((StatusCode::CREATED, Json(response)))
}

/// Get all polls
pub async fn list_polls(
    Extension(app_state): Extension<AppState>,
    session: Session,
) -> Result<impl IntoResponse, PollError> {
    let user_id = get_user_id_from_session(&session).await.ok();

    let polls = db::get_all_polls(&app_state.db)
        .await
        .map_err(|e| PollError::DatabaseError(e.to_string()))?;

    let mut poll_responses = Vec::new();

    for poll in polls {
        let options = db::get_poll_options(&app_state.db, poll.id)
            .await
            .map_err(|e| PollError::DatabaseError(e.to_string()))?;

        let user_voted = if let Some(uid) = user_id {
            db::user_has_voted(&app_state.db, poll.id, uid)
                .await
                .unwrap_or(false)
        } else {
            false
        };

        let option_responses = options
            .into_iter()
            .map(|opt| PollOptionWithVotesResponse {
                id: opt.id,
                text: opt.option_text,
                votes: opt.votes,
            })
            .collect();

        poll_responses.push(PollResponse {
            id: poll.id,
            title: poll.title,
            description: poll.description,
            creator_id: poll.creator_id,
            created_at: poll.created_at.to_rfc3339(),
            closed: poll.closed,
            options: option_responses,
            user_voted,
        });
    }

    Ok((StatusCode::OK, Json(poll_responses)))
}

/// Get a specific poll with all its options and vote counts
pub async fn get_poll(
    Extension(app_state): Extension<AppState>,
    session: Session,
    Path(poll_id): Path<Uuid>,
) -> Result<impl IntoResponse, PollError> {
    let user_id = get_user_id_from_session(&session).await.ok();

    let poll = db::get_poll(&app_state.db, poll_id)
        .await
        .map_err(|e| PollError::DatabaseError(e.to_string()))?
        .ok_or(PollError::PollNotFound)?;

    let options = db::get_poll_options(&app_state.db, poll_id)
        .await
        .map_err(|e| PollError::DatabaseError(e.to_string()))?;

    let user_voted = if let Some(uid) = user_id {
        db::user_has_voted(&app_state.db, poll_id, uid)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let option_responses = options
        .into_iter()
        .map(|opt| PollOptionWithVotesResponse {
            id: opt.id,
            text: opt.option_text,
            votes: opt.votes,
        })
        .collect();

    let response = PollResponse {
        id: poll.id,
        title: poll.title,
        description: poll.description,
        creator_id: poll.creator_id,
        created_at: poll.created_at.to_rfc3339(),
        closed: poll.closed,
        options: option_responses,
        user_voted,
    };

    Ok((StatusCode::OK, Json(response)))
}

/// Cast a vote on a poll option (authenticated users only)
pub async fn vote_on_poll(
    Extension(app_state): Extension<AppState>,
    session: Session,
    Path(poll_id): Path<Uuid>,
    Json(payload): Json<CastVoteRequest>,
) -> Result<impl IntoResponse, PollError> {
    // Verify user is authenticated
    let user_id = get_user_id_from_session(&session).await?;

    // Verify poll exists and is not closed
    let poll = db::get_poll(&app_state.db, poll_id)
        .await
        .map_err(|e| PollError::DatabaseError(e.to_string()))?
        .ok_or(PollError::PollNotFound)?;

    if poll.closed {
        return Err(PollError::PollClosed);
    }

    // Verify option exists and belongs to this poll
    let options = db::get_poll_options(&app_state.db, poll_id)
        .await
        .map_err(|e| PollError::DatabaseError(e.to_string()))?;

    let option_exists = options.iter().any(|opt| opt.id == payload.option_id);
    if !option_exists {
        return Err(PollError::OptionNotFound);
    }

    // Cast the vote
    match db::cast_vote(&app_state.db, poll_id, payload.option_id, user_id).await {
        Ok(_) => {
            let response = VoteResponse {
                success: true,
                message: "Vote recorded successfully".to_string(),
            };
            Ok((StatusCode::OK, Json(response)))
        }
        Err(sqlx::Error::RowNotFound) => Err(PollError::AlreadyVoted),
        Err(e) => Err(PollError::DatabaseError(e.to_string())),
    }
}

/// Close a poll (only creator can close)
pub async fn close_poll(
    Extension(app_state): Extension<AppState>,
    session: Session,
    Path(poll_id): Path<Uuid>,
) -> Result<impl IntoResponse, PollError> {
    // Verify user is authenticated
    let user_id = get_user_id_from_session(&session).await?;

    // Verify poll exists
    let poll = db::get_poll(&app_state.db, poll_id)
        .await
        .map_err(|e| PollError::DatabaseError(e.to_string()))?
        .ok_or(PollError::PollNotFound)?;

    // Verify user is the creator
    if poll.creator_id != user_id {
        return Err(PollError::Unauthorized);
    }

    // Close the poll
    db::close_poll(&app_state.db, poll_id)
        .await
        .map_err(|e| PollError::DatabaseError(e.to_string()))?;

    Ok((
        StatusCode::OK,
        Json(json!({
            "success": true,
            "message": "Poll closed successfully"
        })),
    ))
}
