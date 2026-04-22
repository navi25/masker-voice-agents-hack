use stream_privacy_engine::{models::EntityType, triggers::TriggerMatcher};

#[test]
fn matches_core_triggers() {
    let tokens = vec!["hello".into(), "my".into(), "social".into(), "is".into()];
    assert_eq!(TriggerMatcher::match_entity(&tokens), Some(EntityType::Ssn));
}
