use super::*;

#[test]
pub(super) fn test_apply_escape_octal() {
    // \101 = 0o101 = 65 = 'A'
    assert_eq!(apply_escape_sequences("\\101"), "A");
}
