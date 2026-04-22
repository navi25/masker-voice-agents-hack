use stream_privacy_engine::normalizer::{Normalizer, SpokenFormNormalizer};

#[test]
fn normalizes_spoken_digits_and_double() {
    let n = SpokenFormNormalizer::new();
    assert_eq!(n.normalize_token("one"), vec!["1"]);
    assert_eq!(n.normalize_token("oh"), vec!["0"]);
    assert_eq!(n.normalize_token("doublefive"), vec!["5", "5"]);
}
