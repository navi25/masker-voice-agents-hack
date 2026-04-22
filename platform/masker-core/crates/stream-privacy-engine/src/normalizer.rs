use std::collections::HashMap;

pub trait Normalizer: Send + Sync {
    fn normalize_token(&self, raw: &str) -> Vec<String>;
}

#[derive(Default)]
pub struct SpokenFormNormalizer {
    digit_map: HashMap<&'static str, &'static str>,
}

impl SpokenFormNormalizer {
    pub fn new() -> Self {
        let mut digit_map = HashMap::new();
        digit_map.insert("zero", "0");
        digit_map.insert("oh", "0");
        digit_map.insert("o", "0");
        digit_map.insert("one", "1");
        digit_map.insert("two", "2");
        digit_map.insert("three", "3");
        digit_map.insert("four", "4");
        digit_map.insert("five", "5");
        digit_map.insert("six", "6");
        digit_map.insert("seven", "7");
        digit_map.insert("eight", "8");
        digit_map.insert("nine", "9");
        Self { digit_map }
    }
}

impl Normalizer for SpokenFormNormalizer {
    fn normalize_token(&self, raw: &str) -> Vec<String> {
        let cleaned = raw
            .to_ascii_lowercase()
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_string();
        if cleaned.is_empty() {
            return vec![];
        }

        if let Some(rest) = cleaned.strip_prefix("double") {
            let last = rest.trim_matches(|c: char| !c.is_alphanumeric());
            if let Some(d) = self.digit_map.get(last) {
                return vec![d.to_string(), d.to_string()];
            }
        }

        if let Some(d) = self.digit_map.get(cleaned.as_str()) {
            return vec![d.to_string()];
        }

        if cleaned.chars().all(|c| c.is_ascii_alphanumeric() || c == '/') {
            return vec![cleaned];
        }

        vec![cleaned]
    }
}
