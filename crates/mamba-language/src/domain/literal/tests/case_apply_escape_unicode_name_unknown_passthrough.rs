use super::*;

#[test]
pub(super) fn test_apply_escape_unicode_name_unknown_passthrough() {
    assert_eq!(
        apply_escape_sequences("\\N{UNKNOWN_XYZ}"),
        "\\N{UNKNOWN_XYZ}"
    );
}
