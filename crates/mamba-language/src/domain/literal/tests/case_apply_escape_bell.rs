use super::*;

#[test]
pub(super) fn test_apply_escape_bell() {
    assert_eq!(apply_escape_sequences("\\a"), "\x07");
}
