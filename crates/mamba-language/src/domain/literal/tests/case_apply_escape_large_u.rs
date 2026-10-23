use super::*;

#[test]
pub(super) fn test_apply_escape_large_u() {
    // \U00000041 = 'A'
    assert_eq!(apply_escape_sequences("\\U00000041"), "A");
}
