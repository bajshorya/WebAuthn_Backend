use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PollUpdate {
    pub poll_id: Uuid,
    pub option_id: Uuid,
    pub new_vote_count: i64,
}

#[derive(Debug, Clone)]
pub struct PollCreated {
    pub poll_id: Uuid,
    pub title: String,
    #[allow(dead_code)]
    pub creator_id: Uuid,
}

#[derive(Debug, Clone)]
pub enum SseEvent {
    VoteUpdate(PollUpdate),
    PollCreated(PollCreated),
    PollClosed(Uuid),
}

pub type SseSender = tokio::sync::broadcast::Sender<SseEvent>;
