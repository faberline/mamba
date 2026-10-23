use super::*;

#[test]
pub(super) fn test_apply_escape_null() {
    assert_eq!(apply_escape_sequences("\\0"), "\0");
}
