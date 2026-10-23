use super::*;

#[test]
pub(super) fn test_lex_bytes_triple_dquote() {
    // #1682 — `b"""...` previously matched the single-quoted byte regex
    // `b""` and left the rest as garbage.
    let kinds = lex_kinds("b\"\"\"hello world\"\"\"");
    assert_eq!(kinds, vec![TokenKind::ByteStr("hello world".into())]);
}
