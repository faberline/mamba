use super::*;

#[test]
pub(super) fn test_apply_escape_vertical_tab() {
    assert_eq!(apply_escape_sequences("\\v"), "\x0B");
}
