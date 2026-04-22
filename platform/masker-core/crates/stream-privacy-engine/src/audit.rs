use serde::Serialize;

use crate::{models::SpanCandidate, policy::PolicyDecision};

#[derive(Debug, Serialize)]
pub struct AuditRecord {
    pub timestamp_ms: u64,
    pub entity_type: String,
    pub span_token_ids: Vec<u64>,
    pub contributing_chunk_ids: Vec<String>,
    pub policy_decision: String,
    pub rules_version: String,
    pub model_version: String,
}

impl AuditRecord {
    pub fn from_candidate(ts: u64, c: &SpanCandidate, decision: &PolicyDecision) -> Self {
        Self {
            timestamp_ms: ts,
            entity_type: format!("{:?}", c.entity_type),
            span_token_ids: c.contributing_token_ids.clone(),
            contributing_chunk_ids: c.chunk_ids.clone(),
            policy_decision: decision.decision.clone(),
            rules_version: "deterministic-v1".to_string(),
            model_version: "noop-v0".to_string(),
        }
    }
}
