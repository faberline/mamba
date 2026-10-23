use super::*;

// --- PEP 701 f-string enhancements ---

#[test]
pub(super) fn test_lex_fstring_quote_reuse_single_in_double() {
    // f"result: {d['key']}" — single quotes inside double-quoted expression
    let kinds = lex_kinds("f\"result: {d['key']}\"");
    assert_eq!(kinds, vec![TokenKind::FStr("result: {d['key']}".into())]);
}
