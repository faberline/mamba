use super::*;

// --- Wildcard ---

#[test]
pub(super) fn test_wildcard_pattern() {
    assert!(matches!(parse_pattern("_"), Pattern::Wildcard));
}
