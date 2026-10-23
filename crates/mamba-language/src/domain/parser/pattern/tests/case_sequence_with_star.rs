use super::*;

#[test]
pub(super) fn test_sequence_with_star() {
    match parse_pattern("[a, *rest]") {
        Pattern::Sequence(pats) => {
            assert_eq!(pats.len(), 2);
            assert!(matches!(&pats[0].node, Pattern::Binding(n) if n == "a"));
            match &pats[1].node {
                Pattern::Star(Some(name)) => assert_eq!(name, "rest"),
                other => panic!("expected Star(rest), got {other:?}"),
            }
        }
        other => panic!("expected Sequence, got {other:?}"),
    }
}
