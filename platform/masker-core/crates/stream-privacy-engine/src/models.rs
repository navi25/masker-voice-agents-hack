use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stability {
    Partial,
    Final,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityType {
    Ssn,
    Phone,
    Dob,
    MemberId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssemblerState {
    Idle,
    ExpectingSsn,
    ExpectingPhone,
    ExpectingDob,
    ExpectingMemberId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResolutionStatus {
    Suspected,
    Likely,
    Confirmed,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RedactionStatus {
    NotRedacted,
    Redacted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub id: u64,
    pub raw_text: String,
    pub normalized_text: String,
    pub start_ts_ms: u64,
    pub end_ts_ms: u64,
    pub confidence: f32,
    pub speaker_id: Option<String>,
    pub stability: Stability,
    pub chunk_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanCandidate {
    pub id: u64,
    pub entity_type: EntityType,
    pub state: AssemblerState,
    pub contributing_token_ids: Vec<u64>,
    pub assembled_normalized_value: String,
    pub confidence: f32,
    pub resolution_status: ResolutionStatus,
    pub redaction_status: RedactionStatus,
    pub speaker_id: Option<String>,
    pub chunk_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionEvent {
    pub candidate_id: u64,
    pub entity_type: EntityType,
    pub status: ResolutionStatus,
    pub confidence: f32,
    pub chunk_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionAction {
    pub candidate_id: u64,
    pub entity_type: EntityType,
    pub token_ids: Vec<u64>,
    pub replacement: String,
}
