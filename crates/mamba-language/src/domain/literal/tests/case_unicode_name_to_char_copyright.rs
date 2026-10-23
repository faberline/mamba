use super::*;

#[test]
pub(super) fn test_unicode_name_to_char_copyright() {
    assert_eq!(unicode_name_to_char("COPYRIGHT SIGN"), Some('\u{00A9}'));
}
