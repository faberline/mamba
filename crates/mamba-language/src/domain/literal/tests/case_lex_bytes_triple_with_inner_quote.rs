use super::*;

#[test]
pub(super) fn test_lex_bytes_triple_with_inner_quote() {
    // Single quote inside b""" is preserved verbatim.
    let kinds = lex_kinds("b\"\"\"has 'quote'\"\"\"");
    assert_eq!(kinds, vec![TokenKind::ByteStr("has 'quote'".into())]);
}
