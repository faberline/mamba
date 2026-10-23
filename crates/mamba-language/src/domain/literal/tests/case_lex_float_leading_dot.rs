use super::*;

#[test]
pub(super) fn test_lex_float_leading_dot() {
    let kinds = lex_kinds(".5");
    assert_eq!(kinds, vec![TokenKind::Float(0.5)]);
}
