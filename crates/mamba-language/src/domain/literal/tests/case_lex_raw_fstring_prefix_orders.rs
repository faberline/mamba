use super::*;

#[test]
pub(super) fn test_lex_raw_fstring_prefix_orders() {
    // Both prefix orders lex to a single RawFStr token, not Ident + string.
    assert_eq!(
        lex_kinds("rf\"\\n{1}\""),
        vec![TokenKind::RawFStr("\\n{1}".into())]
    );
    assert_eq!(
        lex_kinds("fr\"{1}\\t\""),
        vec![TokenKind::RawFStr("{1}\\t".into())]
    );
    assert_eq!(
        lex_kinds("rf'\\n{1}'"),
        vec![TokenKind::RawFStr("\\n{1}".into())]
    );
    assert_eq!(
        lex_kinds("fr'{1}\\t'"),
        vec![TokenKind::RawFStr("{1}\\t".into())]
    );
}
