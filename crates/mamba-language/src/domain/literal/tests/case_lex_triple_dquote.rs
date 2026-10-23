use super::*;

// --- Triple-quoted strings ---

#[test]
pub(super) fn test_lex_triple_dquote() {
    let kinds = lex_kinds("\"\"\"multi\nline\"\"\"");
    assert_eq!(kinds, vec![TokenKind::TripleStr("multi\nline".into())]);
}
