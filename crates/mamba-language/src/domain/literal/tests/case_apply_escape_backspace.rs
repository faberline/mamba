use super::*;

#[test]
pub(super) fn test_apply_escape_backspace() {
    assert_eq!(apply_escape_sequences("\\b"), "\x08");
}
