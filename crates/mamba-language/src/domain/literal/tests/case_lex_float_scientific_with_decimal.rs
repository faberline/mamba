use super::*;

#[test]
pub(super) fn test_lex_float_scientific_with_decimal() {
    let kinds = lex_kinds("2.5e3");
    assert_eq!(kinds, vec![TokenKind::Float(2.5e3)]);
}
