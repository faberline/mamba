use super::*;

#[test]
pub(super) fn test_lex_empty_string() {
    let kinds = lex_kinds("\"\"");
    assert_eq!(kinds, vec![TokenKind::Str("".into())]);
}
