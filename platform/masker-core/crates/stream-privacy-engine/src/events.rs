use crate::models::Stability;

#[derive(Debug, Clone)]
pub struct TranscriptEvent {
    pub chunk_id: String,
    pub text: String,
    pub stability: Stability,
    pub start_ts_ms: u64,
    pub end_ts_ms: u64,
    pub confidence: f32,
    pub speaker_id: Option<String>,
    pub received_ts_ms: u64,
}
