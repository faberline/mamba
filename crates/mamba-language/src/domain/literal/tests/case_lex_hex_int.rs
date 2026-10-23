use super::*;

#[test]
pub(super) fn test_lex_hex_int() {
    let kinds = lex_kinds("0xFF");
    assert_eq!(kinds, vec![TokenKind::Int(255)]);
}
