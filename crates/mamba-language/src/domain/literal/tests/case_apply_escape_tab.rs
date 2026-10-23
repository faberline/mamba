use super::*;

#[test]
pub(super) fn test_apply_escape_tab() {
    assert_eq!(apply_escape_sequences("\\t"), "\t");
}
