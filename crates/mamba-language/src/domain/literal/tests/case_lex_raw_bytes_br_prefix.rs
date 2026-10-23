use super::*;

#[test]
pub(super) fn test_lex_raw_bytes_br_prefix() {
    // `br'...'` is a raw-bytes literal — escapes kept literal.
    let kinds = lex_kinds("br'\\g<a1>'");
    assert_eq!(kinds, vec![TokenKind::ByteStr("\\g<a1>".into())]);
    let kinds = lex_kinds("br\"\\n\"");
    assert_eq!(kinds, vec![TokenKind::ByteStr("\\n".into())]);
}
