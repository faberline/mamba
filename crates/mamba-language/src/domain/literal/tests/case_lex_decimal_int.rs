use super::*;

// --- Integer literals ---

#[test]
pub(super) fn test_lex_decimal_int() {
    let kinds = lex_kinds("42");
    assert_eq!(kinds, vec![TokenKind::Int(42)]);
}
