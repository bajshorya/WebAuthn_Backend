use crate::auth::{
    authenticate_user, finish_authentication, finish_register, register_user, start_authentication,
    start_register,
};
use crate::polls::{close_poll, create_poll, get_poll, list_polls, restart_poll, vote_on_poll};
use crate::sse::{all_polls_sse, create_sse_broadcaster, poll_updates_sse};
use crate::startup::AppState;
use axum::{
    Router,
    extract::Extension,
    http::{
        StatusCode,
        header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE},
    },
    response::IntoResponse,
    routing::options,
};
use std::env;
use std::net::SocketAddr;
use std::time::Duration;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::timeout::TimeoutLayer;
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

    let jwt_secret = env::var("JWT_SECRET").expect("JWT_SECRET must be set in env");
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set in env");

    let port = env::var("PORT").unwrap_or_else(|_| "8080".to_string());

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

    let app_state = AppState::new(db_pool.clone(), jwt_secret).await;
    let sse_tx = create_sse_broadcaster();
    let app = Router::new()
        .route(
            "/register_start/:username",
            options(|| async { (StatusCode::OK, "") }).post(start_register),
        )
        .route(
            "/register_finish",
            options(|| async { (StatusCode::OK, "") }).post(finish_register),
        )
        .route(
            "/login_start/:username",
            options(|| async { (StatusCode::OK, "") }).post(start_authentication),
        )
        .route(
            "/login_finish",
            options(|| async { (StatusCode::OK, "") }).post(finish_authentication),
        )
        .route(
            "/register",
            options(|| async { (StatusCode::OK, "") }).post(register_user),
        )
        .route(
            "/login",
            options(|| async { (StatusCode::OK, "") }).post(authenticate_user),
        )
        .route(
            "/polls",
            options(|| async { (StatusCode::OK, "") })
                .post(create_poll)
                .get(list_polls),
        )
        .route(
            "/polls/:poll_id",
            options(|| async { (StatusCode::OK, "") }).get(get_poll),
        )
        .route(
            "/polls/:poll_id/vote",
            options(|| async { (StatusCode::OK, "") }).post(vote_on_poll),
        )
        .route(
            "/polls/:poll_id/close",
            options(|| async { (StatusCode::OK, "") }).post(close_poll),
        )
        .route(
            "/polls/:poll_id/restart",
            options(|| async { (StatusCode::OK, "") }).post(restart_poll),
        )
        .route(
            "/polls/:poll_id/sse",
            options(|| async { (StatusCode::OK, "") }).get(poll_updates_sse),
        )
        .route(
            "/polls/sse",
            options(|| async { (StatusCode::OK, "") }).get(all_polls_sse),
        )
        .layer(
            CorsLayer::new()
                .allow_origin(AllowOrigin::list([
                    "https://polling-app-frontend-rho.vercel.app"
                        .parse()
                        .unwrap(),
                    "https://*.vercel.app".parse().unwrap(),
                    "http://localhost:3000".parse().unwrap(),
                    "http://localhost:5173".parse().unwrap(),
                ]))
                .allow_credentials(true)
                .allow_methods([
                    axum::http::Method::GET,
                    axum::http::Method::POST,
                    axum::http::Method::PUT,
                    axum::http::Method::DELETE,
                    axum::http::Method::OPTIONS,
                    axum::http::Method::PATCH,
                    axum::http::Method::HEAD,
                ])
                .allow_headers([
                    CONTENT_TYPE,
                    ACCEPT,
                    AUTHORIZATION,
                    axum::http::header::ORIGIN,
                    axum::http::header::COOKIE,
                ])
                .expose_headers([
                    axum::http::header::CONTENT_TYPE,
                    AUTHORIZATION,
                    axum::http::header::SET_COOKIE,
                ])
                .max_age(Duration::from_secs(86400)),
        )
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_hours(24 * 30),
        ))
        .layer(Extension(app_state))
        .layer(Extension(sse_tx));

    let addr = SocketAddr::from(([0, 0, 0, 0], port.parse().unwrap()));
    info!("🚀 Server listening on {addr}");

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
