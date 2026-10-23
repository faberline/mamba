use super::*;

#[test]
pub(super) fn test_lex_complex_uppercase() {
    let kinds = lex_kinds("2J");
    assert_eq!(kinds, vec![TokenKind::Complex(2.0)]);
}
