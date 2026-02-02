use crate::db::connection::DbPool;
use std::sync::Arc;
use tokio::time::{Duration, interval};
use tracing::error;
use webauthn_rs::prelude::*;

pub const DATABASE_URL: &str = "postgresql://neondb_owner:npg_NWk9HyIjDcK1@ep-hidden-voice-ahhkgfq9-pooler.c-3.us-east-1.aws.neon.tech/neondb?sslmode=require&channel_binding=require&channel_binding=require&connect_timeout=10&pool_max_connections=20";

#[derive(Clone)]
pub struct AppState {
    pub webauthn: Arc<Webauthn>,
    pub db: DbPool,
}

impl AppState {
    pub async fn new(db: DbPool) -> Self {
        let rp_id = "localhost";
        let rp_origin = Url::parse("http://localhost:3000").expect("Invalid URL");
        let builder = WebauthnBuilder::new(rp_id, &rp_origin).expect("Invalid configuration");
        let builder = builder.rp_name("Axum Webauthn-rs");
        let webauthn = Arc::new(builder.build().expect("Invalid configuration"));

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

        AppState { webauthn, db }
    }
}
