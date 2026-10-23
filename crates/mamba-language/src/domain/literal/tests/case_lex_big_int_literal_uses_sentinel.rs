use super::*;

#[test]
pub(super) fn test_lex_big_int_literal_uses_sentinel() {
    let kinds = lex_kinds("123456789012345678901234567890");
    assert_eq!(kinds, vec![TokenKind::Int(BIG_INT_LITERAL_SENTINEL)]);
}
