use super::*;

#[test]
pub(super) fn test_apply_escape_unicode_name_known() {
    assert_eq!(apply_escape_sequences("\\N{SNOWMAN}"), "\u{2603}");
}
