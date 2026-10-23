use super::*;

#[test]
pub(super) fn test_apply_escape_hex() {
    // \x41 = 'A'
    assert_eq!(apply_escape_sequences("\\x41"), "A");
}
