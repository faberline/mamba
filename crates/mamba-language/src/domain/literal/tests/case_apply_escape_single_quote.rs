use super::*;

#[test]
pub(super) fn test_apply_escape_single_quote() {
    assert_eq!(apply_escape_sequences("\\'"), "'");
}
