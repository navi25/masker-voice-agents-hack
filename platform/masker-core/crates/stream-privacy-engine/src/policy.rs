use crate::models::ResolutionStatus;

#[derive(Debug, Clone)]
pub struct PolicyDecision {
    pub decision: String,
}

pub fn decide(status: ResolutionStatus) -> PolicyDecision {
    let decision = match status {
        ResolutionStatus::Suspected => "suspected_sensitive",
        ResolutionStatus::Likely => "likely_sensitive",
        ResolutionStatus::Confirmed => "confirmed_sensitive",
        ResolutionStatus::Rejected => "rejected",
    };
    PolicyDecision {
        decision: decision.to_string(),
    }
}
