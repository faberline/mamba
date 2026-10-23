use super::*;

#[test]
pub(super) fn test_lex_return_with_annotation() {
    let kinds = lex_kinds("def f() -> int:");
    assert_eq!(
        kinds,
        vec![
            TokenKind::Def,
            TokenKind::Ident,
            TokenKind::LParen,
            TokenKind::RParen,
            TokenKind::Arrow,
            TokenKind::IntType,
            TokenKind::Colon,
        ]
    );
}
