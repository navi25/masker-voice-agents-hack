use crate::models::EntityType;

#[derive(Default)]
pub struct TriggerMatcher;

impl TriggerMatcher {
    pub fn match_entity(tokens: &[String]) -> Option<EntityType> {
        let joined = tokens.join(" ").to_ascii_lowercase();
        if joined.contains("my social is") || joined.contains("my ssn is") {
            return Some(EntityType::Ssn);
        }
        if joined.contains("my phone number is") || joined.contains("phone number") {
            return Some(EntityType::Phone);
        }
        if joined.contains("date of birth") || joined.contains("my dob is") {
            return Some(EntityType::Dob);
        }
        if joined.contains("member id") || joined.contains("insurance number") {
            return Some(EntityType::MemberId);
        }
        None
    }
}
