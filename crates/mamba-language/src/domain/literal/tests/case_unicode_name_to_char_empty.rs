use super::*;

#[test]
pub(super) fn test_unicode_name_to_char_empty() {
    assert_eq!(unicode_name_to_char(""), None);
}
