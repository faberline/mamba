use super::*;

// --- Operators ---

#[test]
pub(super) fn test_lex_arithmetic_operators() {
    let kinds = lex_kinds("+ - * / // % **");
    assert_eq!(
        kinds,
        vec![
            TokenKind::Plus,
            TokenKind::Minus,
            TokenKind::Star,
            TokenKind::Slash,
            TokenKind::DoubleSlash,
            TokenKind::Percent,
            TokenKind::DoubleStar,
        ]
    );
}
