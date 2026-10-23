use super::*;

#[test]
pub(super) fn test_lex_float_trailing_dot_exp() {
    let kinds = lex_kinds("5.e2");
    assert_eq!(kinds, vec![TokenKind::Float(5.0e2)]);
}
