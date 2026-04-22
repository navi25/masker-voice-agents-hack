use crate::models::{DetectionEvent, Token};

pub trait LearnedDetector: Send + Sync {
    fn detect(&self, _tokens: &[Token]) -> Vec<DetectionEvent>;
}

#[derive(Default)]
pub struct NoopLearnedDetector;

impl LearnedDetector for NoopLearnedDetector {
    fn detect(&self, _tokens: &[Token]) -> Vec<DetectionEvent> {
        vec![]
    }
}
