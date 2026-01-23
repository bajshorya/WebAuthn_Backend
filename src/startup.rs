use crate::db::DbPool;
use std::sync::Arc;
use webauthn_rs::prelude::*;

pub const DATABASE_URL: &str = "postgresql://neondb_owner:npg_NWk9HyIjDcK1@ep-hidden-voice-ahhkgfq9-pooler.c-3.us-east-1.aws.neon.tech/neondb?sslmode=require&channel_binding=require";

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
        AppState { webauthn, db }
    }
}
