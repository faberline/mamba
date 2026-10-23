use super::*;

#[test]
pub(super) fn test_lex_u_prefix_string_double() {
    // #1920 — Python 2-compat `u"..."` no-op prefix; must tokenize as a
    // single Str token (previously split into a bare `u` Ident + an
    // orphaned string).
    let kinds = lex_kinds("u\"bar\"");
    assert_eq!(kinds, vec![TokenKind::Str("bar".into())]);
}
