use super::*;

#[test]
pub(super) fn test_lex_int_with_underscores() {
    let kinds = lex_kinds("1_000_000");
    assert_eq!(kinds, vec![TokenKind::Int(1_000_000)]);
}
