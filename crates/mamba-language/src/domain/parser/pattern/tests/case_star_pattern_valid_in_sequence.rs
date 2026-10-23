use super::*;

#[test]
pub(super) fn test_star_pattern_valid_in_sequence() {
    // `case [*rest]:` is valid — star pattern inside a sequence.
    match parse_pattern("[*rest]") {
        Pattern::Sequence(pats) => {
            assert_eq!(pats.len(), 1);
            assert!(matches!(&pats[0].node, Pattern::Star(Some(n)) if n == "rest"));
        }
        other => panic!("expected Sequence([Star(rest)]), got {other:?}"),
    }
}
