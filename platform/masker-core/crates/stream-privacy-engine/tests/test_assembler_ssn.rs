use stream_privacy_engine::{
    engine::SensitiveInfoEngine,
    events::TranscriptEvent,
    models::{ResolutionStatus, Stability},
};

#[test]
fn detects_ssn_across_four_chunks() {
    let mut e = SensitiveInfoEngine::new();
    let chunks = vec![
        ("c1", "my social is", Stability::Final),
        ("c2", "one two three", Stability::Final),
        ("c3", "four five", Stability::Final),
        ("c4", "six seven eight nine", Stability::Final),
    ];
    let mut statuses = vec![];
    for (i, (id, text, s)) in chunks.into_iter().enumerate() {
        let out = e.ingest(TranscriptEvent {
            chunk_id: id.into(),
            text: text.into(),
            stability: s,
            start_ts_ms: (i * 1000) as u64,
            end_ts_ms: (i * 1000 + 900) as u64,
            confidence: 0.95,
            speaker_id: Some("spk1".into()),
            received_ts_ms: (i * 1000) as u64,
        });
        statuses.extend(out.detections.into_iter().map(|d| d.status));
    }
    assert!(statuses.contains(&ResolutionStatus::Confirmed));
}
