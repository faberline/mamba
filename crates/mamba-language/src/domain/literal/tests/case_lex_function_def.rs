use super::*;

// --- Combined expressions ---

#[test]
pub(super) fn test_lex_function_def() {
    let kinds = lex_kinds("def add(a, b):");
    assert_eq!(
        kinds,
        vec![
            TokenKind::Def,
            TokenKind::Ident,
            TokenKind::LParen,
            TokenKind::Ident,
            TokenKind::Comma,
            TokenKind::Ident,
            TokenKind::RParen,
            TokenKind::Colon,
        ]
    );
}
