use crate::auth::{finish_authentication, finish_register, start_authentication, start_register};
use crate::startup::AppState;
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
use tower_sessions::{
    Expiry, MemoryStore, SessionManagerLayer,
    cookie::{SameSite, time::Duration},
};

#[macro_use]
extern crate tracing;

mod auth;
mod error;
mod startup;

// Add this function
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
    // initialize tracing
    tracing_subscriber::fmt::init();

    // Create the app
    let app_state = AppState::new();

    let session_store = MemoryStore::default();

    // build our application with a route
    let app = Router::new()
        .route("/", get(test_page)) // Serve HTML at root
        .route("/register_start/:username", post(start_register))
        .route("/register_finish", post(finish_register))
        .route("/login_start/:username", post(start_authentication))
        .route("/login_finish", post(finish_authentication))
        .layer(Extension(app_state))
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
                .with_secure(false) // TODO: change this to true when running on an HTTPS/production server instead of locally
                .with_expiry(Expiry::OnInactivity(Duration::seconds(360))),
        )
        .fallback(handler_404);

    // run our app with hyper
    // `axum::Server` is a re-export of `hyper::Server`
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
