use super::*;

// --- Sequence pattern ---

#[test]
pub(super) fn test_sequence_pattern_empty() {
    match parse_pattern("[]") {
        Pattern::Sequence(pats) => assert!(pats.is_empty()),
        other => panic!("expected Sequence, got {other:?}"),
    }
}
