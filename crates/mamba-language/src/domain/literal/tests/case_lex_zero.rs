use super::*;

#[test]
pub(super) fn test_lex_zero() {
    let kinds = lex_kinds("0");
    assert_eq!(kinds, vec![TokenKind::Int(0)]);
}
