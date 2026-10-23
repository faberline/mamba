use super::*;

#[test]
pub(super) fn test_lex_logical_operators() {
    let kinds = lex_kinds("and or not");
    assert_eq!(kinds, vec![TokenKind::And, TokenKind::Or, TokenKind::Not,]);
}
