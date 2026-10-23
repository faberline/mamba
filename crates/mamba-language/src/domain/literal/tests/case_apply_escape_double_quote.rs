use super::*;

#[test]
pub(super) fn test_apply_escape_double_quote() {
    assert_eq!(apply_escape_sequences("\\\""), "\"");
}
