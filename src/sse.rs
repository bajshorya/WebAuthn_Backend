use crate::db;
use crate::startup::AppState;
use axum::{
    extract::{Extension, Path},
    response::sse::{Event, KeepAlive, Sse},
};
use futures::stream::Stream;
use serde_json::json;
use std::{convert::Infallible, time::Duration};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PollUpdate {
    pub poll_id: Uuid,
    pub option_id: Uuid,
    pub new_vote_count: i64,
}

#[derive(Debug, Clone)]
pub struct PollCreated {
    pub poll_id: Uuid,
    pub title: String,
    pub creator_id: Uuid,
}

#[derive(Debug, Clone)]
pub enum SseEvent {
    VoteUpdate(PollUpdate),
    PollCreated(PollCreated),
    PollClosed(Uuid),
}

pub type SseSender = tokio::sync::broadcast::Sender<SseEvent>;

pub fn create_sse_broadcaster() -> SseSender {
    tokio::sync::broadcast::channel(100).0
}

pub async fn poll_updates_sse(
    Extension(app_state): Extension<AppState>,
    Extension(sse_tx): Extension<SseSender>,
    Path(poll_id): Path<Uuid>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = sse_tx.subscribe();

    let stream = async_stream::stream! {
        match db::get_poll(&app_state.db, poll_id).await {
            Ok(Some(poll)) => {
                match db::get_poll_options(&app_state.db, poll_id).await {
                    Ok(options) => {
                        let total_votes = options.iter().map(|o| o.votes).sum::<i64>();
                        yield Ok(Event::default()
                            .event("init")
                            .data(json!({
                                "poll": poll,
                                "options": options,
                                "total_votes": total_votes,
                            }).to_string()));
                    }
                    Err(_) => {
                        yield Ok(Event::default()
                            .event("error")
                            .data(json!({"error": "Failed to load poll options"}).to_string()));
                    }
                }
            }
            Ok(None) => {
                yield Ok(Event::default()
                    .event("error")
                    .data(json!({"error": "Poll not found"}).to_string()));
            }
            Err(_) => {
                yield Ok(Event::default()
                    .event("error")
                    .data(json!({"error": "Database error"}).to_string()));
            }
        }

        while let Ok(event) = rx.recv().await {
            match event {
                SseEvent::VoteUpdate(update) if update.poll_id == poll_id => {
                    match db::get_poll_options(&app_state.db, poll_id).await {
                        Ok(options) => {
                            let total_votes = options.iter().map(|o| o.votes).sum::<i64>();
                            yield Ok(Event::default()
                                .event("vote_update")
                                .data(json!({
                                    "options": options,
                                    "total_votes": total_votes,
                                    "updated_option_id": update.option_id,
                                }).to_string()));
                        }
                        Err(_) => {
                        }
                    }
                }
                SseEvent::PollClosed(closed_poll_id) if closed_poll_id == poll_id => {
                    yield Ok(Event::default()
                        .event("poll_closed")
                        .data(json!({"poll_id": poll_id}).to_string()));
                }
                _ => {}
            }
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("keep-alive"),
    )
}
pub async fn all_polls_sse(
    Extension(app_state): Extension<AppState>,
    Extension(sse_tx): Extension<SseSender>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = sse_tx.subscribe();

    let stream = async_stream::stream! {
        {
            let polls_result = db::get_all_polls(&app_state.db).await;
            match polls_result {
                Ok(polls) => {
                    let mut polls_with_details = Vec::new();

                    for poll in polls {
                        let options_result = db::get_poll_options(&app_state.db, poll.id).await;
                        match options_result {
                            Ok(options) => {
                                let total_votes = options.iter().map(|o| o.votes).sum::<i64>();
                                polls_with_details.push(json!({
                                    "id": poll.id,
                                    "title": poll.title,
                                    "description": poll.description,
                                    "creator_id": poll.creator_id,
                                    "created_at": poll.created_at,
                                    "closed": poll.closed,
                                    "options": options,
                                    "total_votes": total_votes,
                                }));
                            }
                            Err(_) => {
                                polls_with_details.push(json!({
                                    "id": poll.id,
                                    "title": poll.title,
                                    "description": poll.description,
                                    "creator_id": poll.creator_id,
                                    "created_at": poll.created_at,
                                    "closed": poll.closed,
                                    "options": [],
                                    "total_votes": 0,
                                }));
                            }
                        }
                    }

                    yield Ok(Event::default()
                        .event("init")
                        .data(json!({"polls": polls_with_details}).to_string()));
                }
                Err(_) => {
                    yield Ok(Event::default()
                        .event("error")
                        .data(json!({"error": "Failed to load polls"}).to_string()));
                }
            }
        }

       
        while let Ok(event) = rx.recv().await {
            match event {
                SseEvent::PollCreated(poll_created) => {
                    let poll_result = db::get_poll(&app_state.db, poll_created.poll_id).await;
                    match poll_result {
                        Ok(Some(poll)) => {
                            let options_result = db::get_poll_options(&app_state.db, poll_created.poll_id).await;
                            match options_result {
                                Ok(options) => {
                                    let total_votes = options.iter().map(|o| o.votes).sum::<i64>();
                                    yield Ok(Event::default()
                                        .event("poll_created")
                                        .data(json!({
                                            "poll": {
                                                "id": poll.id,
                                                "title": poll.title,
                                                "description": poll.description,
                                                "creator_id": poll.creator_id,
                                                "created_at": poll.created_at,
                                                "closed": poll.closed,
                                                "options": options,
                                                "total_votes": total_votes,
                                            },
                                            "poll_id": poll_created.poll_id,
                                            "title": poll_created.title,
                                        }).to_string()));
                                }
                                Err(_) => {
                                
                                    yield Ok(Event::default()
                                        .event("poll_created")
                                        .data(json!({
                                            "poll": {
                                                "id": poll.id,
                                                "title": poll.title,
                                                "description": poll.description,
                                                "creator_id": poll.creator_id,
                                                "created_at": poll.created_at,
                                                "closed": poll.closed,
                                                "options": [],
                                                "total_votes": 0,
                                            },
                                            "poll_id": poll_created.poll_id,
                                            "title": poll_created.title,
                                        }).to_string()));
                                }
                            }
                        }
                        _ => {
                        }
                    }
                }
                SseEvent::VoteUpdate(update) => {
                
                    match db::get_poll(&app_state.db, update.poll_id).await {
                        Ok(Some(poll)) => {
                            match db::get_poll_options(&app_state.db, update.poll_id).await {
                                Ok(options) => {
                                    let total_votes = options.iter().map(|o| o.votes).sum::<i64>();
                                    yield Ok(Event::default()
                                        .event("poll_updated")
                                        .data(json!({
                                            "poll": {
                                                "id": poll.id,
                                                "title": poll.title,
                                                "description": poll.description,
                                                "creator_id": poll.creator_id,
                                                "created_at": poll.created_at,
                                                "closed": poll.closed,
                                                "options": options,
                                                "total_votes": total_votes,
                                            },
                                            "poll_id": update.poll_id,
                                            "updated_option_id": update.option_id,
                                            "new_vote_count": update.new_vote_count,
                                        }).to_string()));
                                }
                                Err(_) => {
                                   
                                    yield Ok(Event::default()
                                        .event("poll_updated")
                                        .data(json!({
                                            "poll": {
                                                "id": poll.id,
                                                "title": poll.title,
                                                "description": poll.description,
                                                "creator_id": poll.creator_id,
                                                "created_at": poll.created_at,
                                                "closed": poll.closed,
                                                "options": [],
                                                "total_votes": 0,
                                            },
                                            "poll_id": update.poll_id,
                                            "updated_option_id": update.option_id,
                                            "new_vote_count": update.new_vote_count,
                                        }).to_string()));
                                }
                            }
                        }
                        _ => {
                            
                        }
                    }
                }
                SseEvent::PollClosed(poll_id) => {
                    yield Ok(Event::default()
                        .event("poll_closed")
                        .data(json!({"poll_id": poll_id}).to_string()));
                }
            }
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("keep-alive"),
    )
}
