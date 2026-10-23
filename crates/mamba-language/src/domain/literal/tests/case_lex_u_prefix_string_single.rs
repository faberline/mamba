use super::*;

#[test]
pub(super) fn test_lex_u_prefix_string_single() {
    let kinds = lex_kinds("u'bar'");
    assert_eq!(kinds, vec![TokenKind::Str("bar".into())]);
}
