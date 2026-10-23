use super::*;

#[test]
pub(super) fn test_lex_bytes_triple_keeps_unicode_escape_raw() {
    let kinds = lex_kinds("b\"\"\"\\u0041\"\"\"");
    assert_eq!(kinds, vec![TokenKind::ByteStr("\\u0041".into())]);
}
