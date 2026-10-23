use super::*;

#[test]
pub(super) fn test_lex_underscore_identifier() {
    let kinds = lex_kinds("_private __dunder__");
    assert_eq!(kinds, vec![TokenKind::Ident, TokenKind::Ident]);
}
