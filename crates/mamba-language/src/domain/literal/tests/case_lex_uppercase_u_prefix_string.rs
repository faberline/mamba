use super::*;

#[test]
pub(super) fn test_lex_uppercase_u_prefix_string() {
    let kinds = lex_kinds("U'bar'");
    assert_eq!(kinds, vec![TokenKind::Str("bar".into())]);
}
