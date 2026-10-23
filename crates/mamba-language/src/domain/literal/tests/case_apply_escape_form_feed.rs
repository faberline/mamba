use super::*;

#[test]
pub(super) fn test_apply_escape_form_feed() {
    assert_eq!(apply_escape_sequences("\\f"), "\x0C");
}
