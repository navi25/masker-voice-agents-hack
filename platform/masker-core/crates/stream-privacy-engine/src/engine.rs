use std::collections::{HashSet, VecDeque};

use crate::{
    assembler::EntityAssembler,
    audit::AuditRecord,
    events::TranscriptEvent,
    learned_detector::{LearnedDetector, NoopLearnedDetector},
    models::{DetectionEvent, RedactionAction, RedactionStatus, ResolutionStatus, Stability, Token},
    normalizer::{Normalizer, SpokenFormNormalizer},
    policy,
    triggers::TriggerMatcher,
};

pub struct EngineOutput {
    pub detections: Vec<DetectionEvent>,
    pub redactions: Vec<RedactionAction>,
    pub audit_records: Vec<AuditRecord>,
}

pub struct SensitiveInfoEngine {
    normalizer: Box<dyn Normalizer>,
    learned: Box<dyn LearnedDetector>,
    committed_tokens: Vec<Token>,
    active_candidates: Vec<crate::models::SpanCandidate>,
    next_token_id: u64,
    next_candidate_id: u64,
    recent_committed_words: VecDeque<String>,
    context_window: usize,
}

impl Default for SensitiveInfoEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl SensitiveInfoEngine {
    pub fn new() -> Self {
        Self {
            normalizer: Box::new(SpokenFormNormalizer::new()),
            learned: Box::new(NoopLearnedDetector),
            committed_tokens: vec![],
            active_candidates: vec![],
            next_token_id: 1,
            next_candidate_id: 1,
            recent_committed_words: VecDeque::new(),
            context_window: 24,
        }
    }

    pub fn ingest(&mut self, event: TranscriptEvent) -> EngineOutput {
        let tokens = self.tokenize(&event);
        let mut detections = vec![];
        let mut redactions = vec![];
        let mut audit_records = vec![];

        if event.stability == Stability::Final {
            for t in &tokens {
                self.recent_committed_words.push_back(t.raw_text.to_ascii_lowercase());
                while self.recent_committed_words.len() > self.context_window {
                    self.recent_committed_words.pop_front();
                }
            }
            self.spawn_candidate_if_triggered(&tokens, &event.speaker_id);

            for t in &tokens {
                for c in &mut self.active_candidates {
                    EntityAssembler::consume_token(c, t);
                }
            }

            for c in &mut self.active_candidates {
                let decision = policy::decide(c.resolution_status);
                detections.push(DetectionEvent {
                    candidate_id: c.id,
                    entity_type: c.entity_type,
                    status: c.resolution_status,
                    confidence: c.confidence,
                    chunk_ids: c.chunk_ids.clone(),
                });

                if c.resolution_status == ResolutionStatus::Confirmed
                    && c.redaction_status == RedactionStatus::NotRedacted
                {
                    c.redaction_status = RedactionStatus::Redacted;
                    redactions.push(RedactionAction {
                        candidate_id: c.id,
                        entity_type: c.entity_type,
                        token_ids: c.contributing_token_ids.clone(),
                        replacement: format!("[REDACTED:{:?}]", c.entity_type),
                    });
                }
                if matches!(
                    c.resolution_status,
                    ResolutionStatus::Likely
                        | ResolutionStatus::Confirmed
                        | ResolutionStatus::Rejected
                        | ResolutionStatus::Suspected
                ) {
                    audit_records.push(AuditRecord::from_candidate(
                        event.received_ts_ms,
                        c,
                        &decision,
                    ));
                }
            }
            self.committed_tokens.extend(tokens);
            detections.extend(self.learned.detect(&self.committed_tokens));
        } else {
            // Provisional path: no irreversible state mutation.
            let mut provisional = self.active_candidates.clone();
            self.spawn_candidate_if_triggered_with_target(&tokens, &event.speaker_id, &mut provisional);
            for t in &tokens {
                for c in &mut provisional {
                    EntityAssembler::consume_token(c, t);
                    detections.push(DetectionEvent {
                        candidate_id: c.id,
                        entity_type: c.entity_type,
                        status: c.resolution_status,
                        confidence: c.confidence,
                        chunk_ids: c.chunk_ids.clone(),
                    });
                }
            }
        }

        EngineOutput {
            detections,
            redactions,
            audit_records,
        }
    }

    pub fn redacted_transcript(&self) -> String {
        let mut redacted = HashSet::new();
        for c in &self.active_candidates {
            if c.redaction_status == RedactionStatus::Redacted {
                for id in &c.contributing_token_ids {
                    redacted.insert(*id);
                }
            }
        }
        let mut out = vec![];
        for t in &self.committed_tokens {
            if redacted.contains(&t.id) {
                out.push("[REDACTED]".to_string());
            } else {
                out.push(t.raw_text.clone());
            }
        }
        out.join(" ")
    }

    fn spawn_candidate_if_triggered(&mut self, tokens: &[Token], speaker_id: &Option<String>) {
        let mut words: Vec<String> = self.recent_committed_words.iter().cloned().collect();
        words.extend(tokens.iter().map(|t| t.raw_text.to_ascii_lowercase()));
        if let Some(entity) = TriggerMatcher::match_entity(&words) {
            self.active_candidates.push(EntityAssembler::new_candidate(
                self.next_candidate_id,
                entity,
                speaker_id.clone(),
            ));
            self.next_candidate_id += 1;
        }
    }

    fn spawn_candidate_if_triggered_with_target(
        &self,
        tokens: &[Token],
        speaker_id: &Option<String>,
        target: &mut Vec<crate::models::SpanCandidate>,
    ) {
        let mut words: Vec<String> = self.recent_committed_words.iter().cloned().collect();
        words.extend(tokens.iter().map(|t| t.raw_text.to_ascii_lowercase()));
        if let Some(entity) = TriggerMatcher::match_entity(&words) {
            target.push(EntityAssembler::new_candidate(999_999, entity, speaker_id.clone()));
        }
    }

    fn tokenize(&mut self, event: &TranscriptEvent) -> Vec<Token> {
        let words: Vec<String> = event
            .text
            .split_whitespace()
            .map(|w| w.to_string())
            .collect();
        let duration = event.end_ts_ms.saturating_sub(event.start_ts_ms).max(1);
        let step = (duration / (words.len().max(1) as u64)).max(1);

        let mut out = vec![];
        for (idx, raw) in words.iter().enumerate() {
            let normalized = self.normalizer.normalize_token(raw);
            for n in normalized {
                let start = event.start_ts_ms + step * idx as u64;
                let end = start + step;
                out.push(Token {
                    id: self.next_token_id,
                    raw_text: raw.clone(),
                    normalized_text: n,
                    start_ts_ms: start,
                    end_ts_ms: end,
                    confidence: event.confidence,
                    speaker_id: event.speaker_id.clone(),
                    stability: event.stability,
                    chunk_id: event.chunk_id.clone(),
                });
                self.next_token_id += 1;
            }
        }
        out
    }
}
