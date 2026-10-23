use super::*;

#[test]
pub(super) fn test_lex_float_scientific() {
    let kinds = lex_kinds("1e10");
    assert_eq!(kinds, vec![TokenKind::Float(1e10)]);
}
