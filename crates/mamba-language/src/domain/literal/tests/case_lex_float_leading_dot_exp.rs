use super::*;

#[test]
pub(super) fn test_lex_float_leading_dot_exp() {
    let kinds = lex_kinds(".5e10");
    assert_eq!(kinds, vec![TokenKind::Float(0.5e10)]);
}
