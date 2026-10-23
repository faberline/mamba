use super::*;

#[test]
pub(super) fn test_lex_bytes_triple_squote_multiline() {
    let kinds = lex_kinds("b'''multi\nline'''");
    assert_eq!(kinds, vec![TokenKind::ByteStr("multi\nline".into())]);
}
