use crate::auth::{finish_authentication, finish_register, start_authentication, start_register};
use crate::polls::{close_poll, create_poll, get_poll, list_polls, vote_on_poll};
use crate::sse::{all_polls_sse, create_sse_broadcaster, poll_updates_sse};
use crate::startup::AppState;
use axum::Json;
use axum::{
    Router,
    extract::Extension,
    http::{
        StatusCode,
        header::{ACCEPT, CONTENT_TYPE},
    },
    response::IntoResponse,
    routing::{get, post},
};
use serde_json::json;
use std::env;
use std::net::SocketAddr;
use std::time::Duration;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::timeout::TimeoutLayer;
use tower_sessions::cookie::SameSite;
use tower_sessions::cookie::time::Duration as CookieDuration;
use tower_sessions::{Expiry, MemoryStore, Session, SessionManagerLayer};
use tower_sessions_sqlx_store::PostgresStore;
use tracing::{error, info};

mod auth;
mod error;
mod polls;
mod sse;
mod startup;
mod db {
    pub mod connection;
    pub mod models;
    pub mod repositories;

    pub use connection::*;
    pub use repositories::*;
}
#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    if std::env::var("RUST_LOG").is_err() {
        unsafe {
            std::env::set_var("RUST_LOG", "INFO");
        }
    }
    tracing_subscriber::fmt::init();
    let db_url = env::var("DATABASE_URL").expect("Database url must be set in env !!");
    let db_pool = match db::init_db(&db_url).await {
        Ok(pool) => {
            info!("Database initialized successfully");
            pool
        }
        Err(e) => {
            error!("Failed to initialize database: {:?}", e);
            panic!("Database initialization failed");
        }
    };

    let app_state = AppState::new(db_pool.clone()).await;
    let session_store = PostgresStore::new(db_pool);

    session_store
        .migrate()
        .await
        .expect("Failed to migrate session table");

    let sse_tx = create_sse_broadcaster();
    let app = Router::new()
        .route("/register_start/:username", post(start_register))
        .route("/register_finish", post(finish_register))
        .route("/login_start/:username", post(start_authentication))
        .route("/login_finish", post(finish_authentication))
        .route("/logout", post(logout))
        .route("/debug/db-stats", get(debug_db_stats))
        .route("/polls", post(create_poll))
        .route("/polls", get(list_polls))
        .route("/polls/:poll_id", get(get_poll))
        .route("/polls/:poll_id/vote", post(vote_on_poll))
        .route("/polls/:poll_id/close", post(close_poll))
        .route("/polls/:poll_id/sse", get(poll_updates_sse))
        .route("/polls/sse", get(all_polls_sse))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_hours(24 * 30),
        ))
        .layer(Extension(app_state))
        .layer(Extension(sse_tx))
        .layer(
            CorsLayer::new()
                .allow_origin(AllowOrigin::list([
                    "https://polldance.vercel.app".parse().unwrap(),
                    "http://localhost:3000".parse().unwrap(),
                    "http://127.0.0.1:3000".parse().unwrap(),
                ]))
                .allow_credentials(true)
                .allow_methods([
                    axum::http::Method::POST,
                    axum::http::Method::GET,
                    axum::http::Method::OPTIONS,
                    axum::http::Method::PUT,
                    axum::http::Method::DELETE,
                ])
                .allow_headers([
                    CONTENT_TYPE,
                    ACCEPT,
                    axum::http::header::ORIGIN,
                    axum::http::header::AUTHORIZATION,
                    axum::http::header::COOKIE,
                ])
                .expose_headers([
                    axum::http::header::SET_COOKIE,
                    axum::http::header::CONTENT_TYPE,
                ])
                .max_age(Duration::from_secs(86400)),
        )
        .layer(
            SessionManagerLayer::new(session_store)
                .with_name("webauthnrs")
                .with_same_site(SameSite::None)
                .with_secure(true)
                .with_expiry(Expiry::OnInactivity(CookieDuration::days(7)))
                .with_http_only(true)
                .with_path("/"),
        );

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    info!("listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Unable to spawn tcp listener");

    axum::serve(listener, app).await.unwrap();
}

#[allow(dead_code)]
async fn handler_404() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "nothing to see here")
}

async fn debug_db_stats(Extension(app_state): Extension<AppState>) -> impl IntoResponse {
    match db::get_pool_stats(&app_state.db).await {
        Ok(stats) => (StatusCode::OK, stats),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Error: {}", e)),
    }
}

async fn logout(session: Session) -> impl IntoResponse {
    session.clear().await;
    Json(json!({
        "success": true,
        "message": "Logged out successfully"
    }))
}
