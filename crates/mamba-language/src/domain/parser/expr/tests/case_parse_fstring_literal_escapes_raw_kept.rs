use super::*;

#[test]
pub(super) fn test_parse_fstring_literal_escapes_raw_kept() {
    // Raw f-string (`rf'...'`/`fr'...'`): backslash escapes stay verbatim.
    let parts = parse_fstring_parts("a\\nb", true).unwrap();
    assert_eq!(parts.len(), 1);
    assert!(matches!(&parts[0], FStringPart::Literal(s) if s == "a\\nb"));
}
