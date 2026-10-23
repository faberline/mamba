use super::*;

#[test]
pub(super) fn test_lex_code_with_comment() {
    let kinds = lex_kinds("42 # answer");
    assert_eq!(kinds, vec![TokenKind::Int(42), TokenKind::Comment]);
}
