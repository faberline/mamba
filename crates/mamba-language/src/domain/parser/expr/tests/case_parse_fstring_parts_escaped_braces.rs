use super::*;

#[test]
pub(super) fn test_parse_fstring_parts_escaped_braces() {
    let parts = parse_fstring_parts("{{literal}}", false).unwrap();
    assert_eq!(parts.len(), 1);
    assert!(matches!(&parts[0], FStringPart::Literal(s) if s == "{literal}"));
}
