use stream_privacy_engine::{
    engine::SensitiveInfoEngine,
    events::TranscriptEvent,
    models::Stability,
};

fn main() {
    let mut engine = SensitiveInfoEngine::new();
    let events = vec![
        TranscriptEvent {
            chunk_id: "c1".into(),
            text: "my social is".into(),
            stability: Stability::Final,
            start_ts_ms: 0,
            end_ts_ms: 800,
            confidence: 0.98,
            speaker_id: Some("caller".into()),
            received_ts_ms: 0,
        },
        TranscriptEvent {
            chunk_id: "c2".into(),
            text: "one two three".into(),
            stability: Stability::Final,
            start_ts_ms: 1000,
            end_ts_ms: 1800,
            confidence: 0.98,
            speaker_id: Some("caller".into()),
            received_ts_ms: 1000,
        },
        TranscriptEvent {
            chunk_id: "c3".into(),
            text: "four five six seven eight nine".into(),
            stability: Stability::Final,
            start_ts_ms: 2000,
            end_ts_ms: 2800,
            confidence: 0.98,
            speaker_id: Some("caller".into()),
            received_ts_ms: 2000,
        },
    ];

    for event in events {
        let out = engine.ingest(event);
        println!("detections: {}", serde_json::to_string_pretty(&out.detections).unwrap());
        println!("redactions: {}", serde_json::to_string_pretty(&out.redactions).unwrap());
        println!("audit: {}", serde_json::to_string_pretty(&out.audit_records).unwrap());
    }

    println!("final redacted transcript: {}", engine.redacted_transcript());
}
