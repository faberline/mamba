use super::*;

#[test]
pub(super) fn test_unicode_name_to_char_space() {
    assert_eq!(unicode_name_to_char("SPACE"), Some(' '));
}
