use super::*;

#[test]
pub(super) fn test_lex_raw_bytes_rb_prefix() {
    // `rb'...'` — same shape as br'', different prefix order.
    let kinds = lex_kinds("rb'\\d+'");
    assert_eq!(kinds, vec![TokenKind::ByteStr("\\d+".into())]);
    let kinds = lex_kinds("rb\"\\d+\"");
    assert_eq!(kinds, vec![TokenKind::ByteStr("\\d+".into())]);
}
