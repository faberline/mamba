use super::*;

#[test]
pub(super) fn test_apply_escape_small_u() {
    // \u0041 = 'A'
    assert_eq!(apply_escape_sequences("\\u0041"), "A");
}
