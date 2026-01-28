use crate::db;
use crate::error::PollError;
use crate::sse::{SseEvent, SseSender};
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
#[derive(Debug, Deserialize)]
pub struct CreatePollRequest {
    pub title: String,
    pub description: Option<String>,
    pub options: Vec<String>,
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
    pub current_user_id: Option<Uuid>,
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

async fn require_auth(session: &Session) -> Result<Uuid, PollError> {
    session
        .get::<Uuid>("user_id")
        .await
        .map_err(|_| PollError::Unauthorized)?
        .ok_or(PollError::Unauthorized)
}

async fn get_user_id_from_session(session: &Session) -> Result<Uuid, PollError> {
    session
        .get::<Uuid>("user_id")
        .await
        .map_err(|_| PollError::Unauthorized)?
        .ok_or(PollError::Unauthorized)
}

pub async fn create_poll(
    Extension(app_state): Extension<AppState>,
    Extension(sse_tx): Extension<SseSender>,
    session: Session,
    Json(payload): Json<CreatePollRequest>,
) -> Result<impl IntoResponse, PollError> {
    let user_id = get_user_id_from_session(&session).await?;

    if payload.title.is_empty() || payload.options.is_empty() {
        return Err(PollError::InvalidRequest);
    }

    if payload.options.len() < 2 {
        return Err(PollError::InvalidRequest);
    }

    let poll_id = db::create_poll(
        &app_state.db,
        user_id,
        &payload.title,
        payload.description.as_deref(),
    )
    .await
    .map_err(|e| PollError::DatabaseError(e.to_string()))?;

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

    let _ = sse_tx.send(SseEvent::PollCreated(crate::sse::PollCreated {
        poll_id,
        title: payload.title.clone(),
        creator_id: user_id,
    }));

    let response = CreatePollResponse {
        poll_id,
        title: payload.title,
        description: payload.description,
        options: option_responses,
    };

    Ok((StatusCode::CREATED, Json(response)))
}
pub async fn list_polls(
    Extension(app_state): Extension<AppState>,
    session: Session,
) -> Result<impl IntoResponse, PollError> {
    let user_id = require_auth(&session).await?;
    let polls = db::get_all_polls(&app_state.db)
        .await
        .map_err(|e| PollError::DatabaseError(e.to_string()))?;

    let mut poll_responses = Vec::new();

    for poll in polls {
        let options = db::get_poll_options(&app_state.db, poll.id)
            .await
            .map_err(|e| PollError::DatabaseError(e.to_string()))?;

        let user_voted = db::user_has_voted(&app_state.db, poll.id, user_id)
            .await
            .unwrap_or(false);
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
            current_user_id: Some(user_id),
        });
    }

    Ok((StatusCode::OK, Json(poll_responses)))
}

pub async fn get_poll(
    Extension(app_state): Extension<AppState>,
    session: Session,
    Path(poll_id): Path<Uuid>,
) -> Result<impl IntoResponse, PollError> {
    let user_id = require_auth(&session).await?;
    let poll = db::get_poll(&app_state.db, poll_id)
        .await
        .map_err(|e| PollError::DatabaseError(e.to_string()))?
        .ok_or(PollError::PollNotFound)?;

    let options = db::get_poll_options(&app_state.db, poll_id)
        .await
        .map_err(|e| PollError::DatabaseError(e.to_string()))?;

    let user_voted = db::user_has_voted(&app_state.db, poll_id, user_id)
        .await
        .unwrap_or(false);

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
        current_user_id: Some(user_id),
    };

    Ok((StatusCode::OK, Json(response)))
}

pub async fn vote_on_poll(
    Extension(app_state): Extension<AppState>,
    Extension(sse_tx): Extension<SseSender>,
    session: Session,
    Path(poll_id): Path<Uuid>,
    Json(payload): Json<CastVoteRequest>,
) -> Result<impl IntoResponse, PollError> {
    let user_id = require_auth(&session).await?;

    let poll = db::get_poll(&app_state.db, poll_id)
        .await
        .map_err(|e| PollError::DatabaseError(e.to_string()))?
        .ok_or(PollError::PollNotFound)?;

    if poll.closed {
        return Err(PollError::PollClosed);
    }

    let options = db::get_poll_options(&app_state.db, poll_id)
        .await
        .map_err(|e| PollError::DatabaseError(e.to_string()))?;

    let option_exists = options.iter().any(|opt| opt.id == payload.option_id);
    if !option_exists {
        return Err(PollError::OptionNotFound);
    }

    match db::cast_vote(&app_state.db, poll_id, payload.option_id, user_id).await {
        Ok(_) => {
            let updated_options = db::get_poll_options(&app_state.db, poll_id)
                .await
                .map_err(|e| PollError::DatabaseError(e.to_string()))?;

            if let Some(updated_option) = updated_options.iter().find(|o| o.id == payload.option_id)
            {
                let _ = sse_tx.send(crate::sse::SseEvent::VoteUpdate(crate::sse::PollUpdate {
                    poll_id,
                    option_id: payload.option_id,
                    new_vote_count: updated_option.votes,
                }));

                println!(
                    "✅ Broadcasted vote update for poll {} (option {} has {} votes)",
                    poll_id, payload.option_id, updated_option.votes
                );
            }

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
pub async fn close_poll(
    Extension(app_state): Extension<AppState>,
    Extension(sse_tx): Extension<SseSender>,
    session: Session,
    Path(poll_id): Path<Uuid>,
) -> Result<impl IntoResponse, PollError> {
    let user_id = require_auth(&session).await?;

    let poll = db::get_poll(&app_state.db, poll_id)
        .await
        .map_err(|e| PollError::DatabaseError(e.to_string()))?
        .ok_or(PollError::PollNotFound)?;

    if poll.creator_id != user_id {
        return Err(PollError::Unauthorized);
    }

    db::close_poll(&app_state.db, poll_id)
        .await
        .map_err(|e| PollError::DatabaseError(e.to_string()))?;

    let _ = sse_tx.send(SseEvent::PollClosed(poll_id));

    Ok((
        StatusCode::OK,
        Json(json!({
            "success": true,
            "message": "Poll closed successfully"
        })),
    ))
}
