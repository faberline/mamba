use super::*;

#[test]
pub(super) fn test_lex_hex_with_underscores() {
    let kinds = lex_kinds("0xFF_FF");
    assert_eq!(kinds, vec![TokenKind::Int(0xFFFF)]);
}
