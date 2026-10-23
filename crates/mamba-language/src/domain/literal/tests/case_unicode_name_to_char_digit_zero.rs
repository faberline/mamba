use super::*;

#[test]
pub(super) fn test_unicode_name_to_char_digit_zero() {
    assert_eq!(unicode_name_to_char("DIGIT ZERO"), Some('0'));
}
