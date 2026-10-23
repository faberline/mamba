use super::*;

#[test]
pub(super) fn test_lex_string_with_escape() {
    // The lexer processes escape sequences: \" → " at lex time.
    let kinds = lex_kinds("\"he\\\"llo\"");
    assert_eq!(kinds, vec![TokenKind::Str("he\"llo".into())]);
}
