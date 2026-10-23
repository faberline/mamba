use super::*;

#[test]
pub(super) fn test_apply_escape_no_escape_passthrough() {
    assert_eq!(apply_escape_sequences("hello"), "hello");
}
