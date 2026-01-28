use crate::auth::{finish_authentication, finish_register, start_authentication, start_register};
use crate::polls::{close_poll, create_poll, get_poll, list_polls, vote_on_poll};
use crate::sse::{all_polls_sse, create_sse_broadcaster, poll_updates_sse};
use crate::startup::{AppState, DATABASE_URL};
use axum::{
    Router,
    extract::Extension,
    http::{
        StatusCode,
        header::{ACCEPT, CONTENT_TYPE},
    },
    response::{Html, IntoResponse},
    routing::{get, post},
};
use std::net::SocketAddr;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_sessions::cookie::SameSite;
use tower_sessions::cookie::time::Duration;
use tower_sessions::{Expiry, MemoryStore, SessionManagerLayer};
use tracing::{error, info};

mod auth;
mod db;
mod error;
mod polls;
mod sse;
mod startup;
async fn test_page() -> Html<&'static str> {
    Html(include_str!("../webauthn_test.html"))
}

#[tokio::main]
async fn main() {
    if std::env::var("RUST_LOG").is_err() {
        unsafe {
            std::env::set_var("RUST_LOG", "INFO");
        }
    }
    tracing_subscriber::fmt::init();

    let db_pool = match db::init_db(DATABASE_URL).await {
        Ok(pool) => {
            info!("Database initialized successfully");
            pool
        }
        Err(e) => {
            error!("Failed to initialize database: {:?}", e);
            panic!("Database initialization failed");
        }
    };

    let app_state = AppState::new(db_pool).await;

    let session_store = MemoryStore::default();
    let sse_tx = create_sse_broadcaster();

    let app = Router::new()
        .route("/", get(test_page))
        .route("/register_start/:username", post(start_register))
        .route("/register_finish", post(finish_register))
        .route("/login_start/:username", post(start_authentication))
        .route("/login_finish", post(finish_authentication))
        .route("/polls", post(create_poll))
        .route("/polls", get(list_polls))
        .route("/polls/:poll_id", get(get_poll))
        .route("/polls/:poll_id/vote", post(vote_on_poll))
        .route("/polls/:poll_id/close", post(close_poll))
        .route("/polls/:poll_id/sse", get(poll_updates_sse))
        .route("/polls/sse", get(all_polls_sse))
        .layer(Extension(app_state))
        .layer(Extension(sse_tx)) 
        .layer(
            CorsLayer::new()
                .allow_origin(AllowOrigin::mirror_request())
                .allow_credentials(true)
                .allow_methods([
                    axum::http::Method::POST,
                    axum::http::Method::GET,
                    axum::http::Method::OPTIONS,
                ])
                .allow_headers([CONTENT_TYPE, ACCEPT]),
        )
        .layer(
            SessionManagerLayer::new(session_store)
                .with_name("webauthnrs")
                .with_same_site(SameSite::Lax)
                .with_secure(false) 
                .with_expiry(Expiry::OnInactivity(Duration::days(7))) 
                .with_http_only(true), 
        );

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    info!("listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Unable to spawn tcp listener");

    axum::serve(listener, app).await.unwrap();
}

async fn handler_404() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "nothing to see here")
}
