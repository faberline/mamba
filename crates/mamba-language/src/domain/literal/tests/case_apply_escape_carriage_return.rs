use super::*;

#[test]
pub(super) fn test_apply_escape_carriage_return() {
    assert_eq!(apply_escape_sequences("\\r"), "\r");
}
