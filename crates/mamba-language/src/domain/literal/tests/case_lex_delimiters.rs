use super::*;

// --- Delimiters ---

#[test]
pub(super) fn test_lex_delimiters() {
    let kinds = lex_kinds("( ) [ ] { } : , . ;");
    assert_eq!(
        kinds,
        vec![
            TokenKind::LParen,
            TokenKind::RParen,
            TokenKind::LBracket,
            TokenKind::RBracket,
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::Colon,
            TokenKind::Comma,
            TokenKind::Dot,
            TokenKind::Semicolon,
        ]
    );
}
