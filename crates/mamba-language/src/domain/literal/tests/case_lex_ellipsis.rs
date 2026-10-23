use super::*;

// --- Ellipsis ---

#[test]
pub(super) fn test_lex_ellipsis() {
    let kinds = lex_kinds("...");
    assert_eq!(kinds, vec![TokenKind::Ellipsis]);
}
