use super::*;

#[test]
pub(super) fn test_apply_escape_newline() {
    assert_eq!(apply_escape_sequences("\\n"), "\n");
}
