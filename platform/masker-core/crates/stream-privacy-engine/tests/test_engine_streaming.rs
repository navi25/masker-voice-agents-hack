use stream_privacy_engine::{
    engine::SensitiveInfoEngine,
    events::TranscriptEvent,
    models::{ResolutionStatus, Stability},
};

fn ev(id: &str, text: &str, stability: Stability, speaker: &str, ts: u64) -> TranscriptEvent {
    TranscriptEvent {
        chunk_id: id.into(),
        text: text.into(),
        stability,
        start_ts_ms: ts,
        end_ts_ms: ts + 900,
        confidence: 0.9,
        speaker_id: Some(speaker.into()),
        received_ts_ms: ts,
    }
}

#[test]
fn partial_revised_by_final() {
    let mut e = SensitiveInfoEngine::new();
    let p = e.ingest(ev("c1", "my social is one two", Stability::Partial, "a", 0));
    assert!(p.redactions.is_empty());
    let f = e.ingest(ev("c1f", "my social is", Stability::Final, "a", 1000));
    assert!(f.redactions.is_empty());
}

#[test]
fn false_positive_prices_rejected() {
    let mut e = SensitiveInfoEngine::new();
    let out = e.ingest(ev("c1", "that costs one two three dollars", Stability::Final, "a", 0));
    assert!(!out.detections.iter().any(|d| d.status == ResolutionStatus::Confirmed));
}

#[test]
fn trigger_then_later_value() {
    let mut e = SensitiveInfoEngine::new();
    e.ingest(ev("c1", "member id", Stability::Final, "a", 0));
    let out = e.ingest(ev("c2", "a b one two three four", Stability::Final, "a", 1000));
    assert!(out.detections.iter().any(|d| d.status == ResolutionStatus::Confirmed));
}

#[test]
fn speaker_change_interrupts_candidate() {
    let mut e = SensitiveInfoEngine::new();
    e.ingest(ev("c1", "my ssn is", Stability::Final, "a", 0));
    let out = e.ingest(ev("c2", "one two three four five", Stability::Final, "b", 1000));
    assert!(out.detections.iter().any(|d| d.status == ResolutionStatus::Rejected));
}
