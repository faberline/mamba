use super::*;

#[test]
pub(super) fn test_unicode_name_to_char_latin_a() {
    assert_eq!(unicode_name_to_char("LATIN SMALL LETTER A"), Some('a'));
}
