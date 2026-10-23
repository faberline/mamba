use super::*;

#[test]
pub(super) fn test_lex_binary_int() {
    let kinds = lex_kinds("0b1010");
    assert_eq!(kinds, vec![TokenKind::Int(10)]);
}
