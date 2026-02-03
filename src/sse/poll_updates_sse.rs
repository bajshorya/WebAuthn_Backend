use crate::db;
use crate::sse::models::{SseEvent, SseSender};
use crate::startup::AppState;
use axum::{
    extract::{Extension, Path},
    response::sse::{Event, KeepAlive, Sse},
};
use futures::stream::Stream;
use serde_json::json;
use std::{convert::Infallible, time::Duration};
use uuid::Uuid;

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
                        let total_votes = options.iter().map(|o| o.votes).sum::<i32>();
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
                            let total_votes = options.iter().map(|o| o.votes).sum::<i32>();
                            yield Ok(Event::default()
                                .event("vote_update")
                                .data(json!({
                                    "options": options,
                                    "total_votes": total_votes,
                                    "updated_option_id": update.option_id,
                                }).to_string()));
                        }
                        Err(_) => {
                            // Silently continue on error
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
