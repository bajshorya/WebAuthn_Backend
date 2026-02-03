use crate::db::connection::DbPool;
use std::sync::Arc;
use tokio::time::{Duration, interval};
use tracing::error;
use webauthn_rs::prelude::*;

#[derive(Clone)]
pub struct AppState {
    pub webauthn: Arc<Webauthn>,
    pub db: DbPool,
    pub jwt_secret: String,
}

impl AppState {
    pub async fn new(db: DbPool, jwt_secret: String) -> Self {
        let rp_id = "localhost";
        let rp_origin = Url::parse("http://localhost:3000").expect("Invalid URL");
        let builder = WebauthnBuilder::new(rp_id, &rp_origin).expect("Invalid configuration");
        let builder = builder.rp_name("Axum Webauthn-rs");
        let webauthn = Arc::new(builder.build().expect("Invalid configuration"));

        // Database connection health check
        let db_clone = db.clone();
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                match db_clone.acquire().await {
                    Ok(conn) => {
                        drop(conn);
                    }
                    Err(e) => {
                        error!("Database connection health check failed: {}", e);
                    }
                }
            }
        });

        AppState {
            webauthn,
            db,
            jwt_secret,
        }
    }
}
