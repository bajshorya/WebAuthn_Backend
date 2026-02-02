use crate::sse::models::{SseEvent, SseSender};
use tokio::sync::broadcast;

pub fn create_sse_broadcaster() -> broadcast::Sender<SseEvent> {
    let (tx, _rx) = broadcast::channel(100);
    tx
}
