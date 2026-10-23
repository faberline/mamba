use super::*;

#[test]
pub(super) fn test_unicode_name_to_char_null() {
    assert_eq!(unicode_name_to_char("NULL"), Some('\0'));
}
