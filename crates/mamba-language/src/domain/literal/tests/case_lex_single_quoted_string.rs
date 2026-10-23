use super::*;

#[test]
pub(super) fn test_lex_single_quoted_string() {
    let kinds = lex_kinds("'world'");
    assert_eq!(kinds, vec![TokenKind::Str("world".into())]);
}
