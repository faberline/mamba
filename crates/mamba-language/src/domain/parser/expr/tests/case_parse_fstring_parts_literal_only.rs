use super::*;

// --- f-string helpers ---

#[test]
pub(super) fn test_parse_fstring_parts_literal_only() {
    let parts = parse_fstring_parts("hello world", false).unwrap();
    assert_eq!(parts.len(), 1);
    assert!(matches!(&parts[0], FStringPart::Literal(s) if s == "hello world"));
}
