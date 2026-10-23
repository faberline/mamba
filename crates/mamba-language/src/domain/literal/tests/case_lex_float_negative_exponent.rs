use super::*;

#[test]
pub(super) fn test_lex_float_negative_exponent() {
    let kinds = lex_kinds("1e-5");
    assert_eq!(kinds, vec![TokenKind::Float(1e-5)]);
}
