use super::*;

#[test]
pub(super) fn test_lex_octal_int() {
    let kinds = lex_kinds("0o17");
    assert_eq!(kinds, vec![TokenKind::Int(15)]);
}
