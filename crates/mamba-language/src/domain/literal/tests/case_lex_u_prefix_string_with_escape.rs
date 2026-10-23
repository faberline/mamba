use super::*;

#[test]
pub(super) fn test_lex_u_prefix_string_with_escape() {
    // The prefix is a no-op — escape processing still applies, same as
    // an unprefixed string.
    let kinds = lex_kinds("u\"he\\\"llo\"");
    assert_eq!(kinds, vec![TokenKind::Str("he\"llo".into())]);
}
