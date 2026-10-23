use super::*;

#[test]
pub(super) fn test_lex_assignment() {
    let kinds = lex_kinds("x = 42");
    assert_eq!(
        kinds,
        vec![TokenKind::Ident, TokenKind::Eq, TokenKind::Int(42),]
    );
}
