use super::*;

#[test]
pub(super) fn test_parse_fstring_literal_escapes_cooked() {
    // Non-raw f-string: literal runs get Python escape processing, so
    // `\n` collapses to a newline (matching `str` literals).
    let parts = parse_fstring_parts("a\\nb", false).unwrap();
    assert_eq!(parts.len(), 1);
    assert!(matches!(&parts[0], FStringPart::Literal(s) if s == "a\nb"));
}
