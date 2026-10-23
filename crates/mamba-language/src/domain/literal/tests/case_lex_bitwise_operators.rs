use super::*;

#[test]
pub(super) fn test_lex_bitwise_operators() {
    let kinds = lex_kinds("& ^ ~ << >>");
    assert_eq!(
        kinds,
        vec![
            TokenKind::Amp,
            TokenKind::Caret,
            TokenKind::Tilde,
            TokenKind::LShift,
            TokenKind::RShift,
        ]
    );
}
