use super::*;

#[test]
pub(super) fn test_unicode_name_to_char_unknown() {
    assert_eq!(unicode_name_to_char("UNKNOWN NAME XYZ"), None);
}
