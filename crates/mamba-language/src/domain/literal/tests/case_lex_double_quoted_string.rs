use super::*;

// --- String literals ---

#[test]
pub(super) fn test_lex_double_quoted_string() {
    let kinds = lex_kinds("\"hello\"");
    assert_eq!(kinds, vec![TokenKind::Str("hello".into())]);
}
