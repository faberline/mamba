use super::*;

// --- Complex literals ---

#[test]
pub(super) fn test_lex_complex_int() {
    let kinds = lex_kinds("5j");
    assert_eq!(kinds, vec![TokenKind::Complex(5.0)]);
}
