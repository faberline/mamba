use super::*;

// --- unicode_name_to_char ---

#[test]
pub(super) fn test_unicode_name_to_char_snowman() {
    assert_eq!(unicode_name_to_char("SNOWMAN"), Some('\u{2603}'));
}
