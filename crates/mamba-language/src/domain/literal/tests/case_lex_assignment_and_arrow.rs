use super::*;

#[test]
pub(super) fn test_lex_assignment_and_arrow() {
    let kinds = lex_kinds("= ->");
    assert_eq!(kinds, vec![TokenKind::Eq, TokenKind::Arrow]);
}
