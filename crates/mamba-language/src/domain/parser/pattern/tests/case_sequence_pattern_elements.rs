use super::*;

#[test]
pub(super) fn test_sequence_pattern_elements() {
    match parse_pattern("[a, b, c]") {
        Pattern::Sequence(pats) => {
            assert_eq!(pats.len(), 3);
            assert!(matches!(&pats[0].node, Pattern::Binding(n) if n == "a"));
            assert!(matches!(&pats[1].node, Pattern::Binding(n) if n == "b"));
            assert!(matches!(&pats[2].node, Pattern::Binding(n) if n == "c"));
        }
        other => panic!("expected Sequence, got {other:?}"),
    }
}
