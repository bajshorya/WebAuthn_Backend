use crate::db::connection::DbPool;
use std::{env, sync::Arc};
use tokio::time::{Duration, interval};
use tracing::{error, info};
use webauthn_rs::prelude::*;

#[derive(Clone)]
pub struct AppState {
    pub webauthn: Arc<Webauthn>,
    pub db: DbPool,
    pub jwt_secret: String,
}

impl AppState {
    pub async fn new(db: DbPool, jwt_secret: String) -> Self {
        let frontend_url =
            env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());

        let rp_origin = Url::parse(&frontend_url).expect("Invalid FRONTEND_URL format");

        let rp_id = rp_origin
            .host_str()
            .expect("Could not extract host from FRONTEND_URL")
            .to_string();

        let rp_id = rp_id.split(':').next().unwrap().to_string();

        info!("WebAuthn configured with:");
        info!("  RP ID: {}", rp_id);
        info!("  RP Origin: {}", rp_origin);

        let builder =
            WebauthnBuilder::new(&rp_id, &rp_origin).expect("Invalid WebAuthn configuration");

        let builder = builder.rp_name("Polling App");
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

        AppState {
            webauthn,
            db,
            jwt_secret,
        }
    }
}
