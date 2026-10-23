use super::*;

// --- Whitespace skipping ---

#[test]
pub(super) fn test_lex_skips_spaces_and_tabs() {
    let kinds = lex_kinds("  42  \t  99  ");
    assert_eq!(kinds, vec![TokenKind::Int(42), TokenKind::Int(99)]);
}
