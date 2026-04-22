use crate::models::{
    AssemblerState, EntityType, RedactionStatus, ResolutionStatus, SpanCandidate, Token,
};

#[derive(Default)]
pub struct EntityAssembler;

impl EntityAssembler {
    pub fn new_candidate(id: u64, entity_type: EntityType, speaker_id: Option<String>) -> SpanCandidate {
        let state = match entity_type {
            EntityType::Ssn => AssemblerState::ExpectingSsn,
            EntityType::Phone => AssemblerState::ExpectingPhone,
            EntityType::Dob => AssemblerState::ExpectingDob,
            EntityType::MemberId => AssemblerState::ExpectingMemberId,
        };
        SpanCandidate {
            id,
            entity_type,
            state,
            contributing_token_ids: vec![],
            assembled_normalized_value: String::new(),
            confidence: 0.0,
            resolution_status: ResolutionStatus::Suspected,
            redaction_status: RedactionStatus::NotRedacted,
            speaker_id,
            chunk_ids: vec![],
        }
    }

    pub fn consume_token(candidate: &mut SpanCandidate, token: &Token) {
        if candidate.resolution_status == ResolutionStatus::Confirmed
            || candidate.resolution_status == ResolutionStatus::Rejected
        {
            return;
        }

        if let Some(s) = &candidate.speaker_id {
            if token.speaker_id.as_ref() != Some(s) {
                candidate.resolution_status = ResolutionStatus::Rejected;
                return;
            }
        }

        let normalized = token.normalized_text.as_str();
        let accepted = match candidate.entity_type {
            EntityType::Ssn | EntityType::Phone => normalized.chars().all(|c| c.is_ascii_digit()),
            EntityType::Dob => normalized.chars().all(|c| c.is_ascii_digit() || c == '/'),
            EntityType::MemberId => normalized.chars().all(|c| c.is_ascii_alphanumeric()),
        };

        if accepted {
            candidate.contributing_token_ids.push(token.id);
            candidate.assembled_normalized_value.push_str(normalized);
            candidate.chunk_ids.push(token.chunk_id.clone());
            candidate.confidence = candidate.confidence.max(token.confidence);
        }

        let len = candidate.assembled_normalized_value.chars().count();
        candidate.resolution_status = match candidate.entity_type {
            EntityType::Ssn if len >= 9 => ResolutionStatus::Confirmed,
            EntityType::Ssn if len >= 5 => ResolutionStatus::Likely,
            EntityType::Ssn if len > 0 => ResolutionStatus::Suspected,
            EntityType::Phone if len >= 10 => ResolutionStatus::Confirmed,
            EntityType::Phone if len >= 7 => ResolutionStatus::Likely,
            EntityType::Phone if len > 0 => ResolutionStatus::Suspected,
            EntityType::Dob if len >= 8 => ResolutionStatus::Confirmed,
            EntityType::Dob if len >= 4 => ResolutionStatus::Likely,
            EntityType::Dob if len > 0 => ResolutionStatus::Suspected,
            EntityType::MemberId if len >= 6 => ResolutionStatus::Confirmed,
            EntityType::MemberId if len >= 4 => ResolutionStatus::Likely,
            EntityType::MemberId if len > 0 => ResolutionStatus::Suspected,
            _ => candidate.resolution_status,
        };
    }
}
