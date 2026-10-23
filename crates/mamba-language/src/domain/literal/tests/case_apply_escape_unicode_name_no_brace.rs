use super::*;

#[test]
pub(super) fn test_apply_escape_unicode_name_no_brace() {
    assert_eq!(apply_escape_sequences("\\N"), "\\N");
}
