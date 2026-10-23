use super::*;

#[test]
pub(super) fn test_lex_augmented_assignment() {
    let kinds = lex_kinds("+= -= *= /= //= %= **=");
    assert_eq!(
        kinds,
        vec![
            TokenKind::PlusEq,
            TokenKind::MinusEq,
            TokenKind::StarEq,
            TokenKind::SlashEq,
            TokenKind::DoubleSlashEq,
            TokenKind::PercentEq,
            TokenKind::DoubleStarEq,
        ]
    );
}
