use super::*;

#[test]
pub(super) fn test_display_string_variants() {
    assert_eq!(format!("{}", TokenKind::FStr("x".into())), "f\"x\"");
    assert_eq!(format!("{}", TokenKind::RawStr("r".into())), "r\"r\"");
    assert_eq!(format!("{}", TokenKind::ByteStr("b".into())), "b\"b\"");
    assert_eq!(
        format!("{}", TokenKind::TripleStr("t".into())),
        "\"\"\"t\"\"\""
    );
}
