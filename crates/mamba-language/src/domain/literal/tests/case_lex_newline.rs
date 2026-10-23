use super::*;

// --- Newline ---

#[test]
pub(super) fn test_lex_newline() {
    let kinds = lex_kinds("a\nb");
    assert_eq!(
        kinds,
        vec![TokenKind::Ident, TokenKind::Newline, TokenKind::Ident,]
    );
}
