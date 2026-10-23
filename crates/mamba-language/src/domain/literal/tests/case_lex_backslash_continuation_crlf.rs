use super::*;

#[test]
pub(super) fn test_lex_backslash_continuation_crlf() {
    // CRLF variant — \\<CR><LF>
    let kinds = lex_kinds("a \\\r\n    b");
    assert_eq!(kinds, vec![TokenKind::Ident, TokenKind::Ident]);
}
