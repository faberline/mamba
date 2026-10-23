use super::*;

// --- Tuple sequence pattern (#827) ---

#[test]
pub(super) fn test_tuple_sequence_pattern() {
    // (a, b) should parse as Pattern::Sequence, not a literal tuple
    match parse_pattern("(a, b)") {
        Pattern::Sequence(v) => assert_eq!(v.len(), 2),
        other => panic!("expected Sequence, got {other:?}"),
    }

    // () → empty sequence
    match parse_pattern("()") {
        Pattern::Sequence(v) => assert!(v.is_empty()),
        other => panic!("expected empty Sequence, got {other:?}"),
    }

    // (a,) → single-element sequence
    match parse_pattern("(a,)") {
        Pattern::Sequence(v) => assert_eq!(v.len(), 1),
        other => panic!("expected single-element Sequence, got {other:?}"),
    }
}
