use super::*;

#[test]
pub(super) fn test_lex_identifier_with_numbers() {
    let kinds = lex_kinds("x2");
    assert_eq!(kinds, vec![TokenKind::Ident]);
}
