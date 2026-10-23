use super::*;

#[test]
pub(super) fn test_lex_other_keywords() {
    let kinds = lex_kinds("lambda del assert global nonlocal is type");
    assert_eq!(
        kinds,
        vec![
            TokenKind::Lambda,
            TokenKind::Del,
            TokenKind::Assert,
            TokenKind::Global,
            TokenKind::Nonlocal,
            TokenKind::Is,
            TokenKind::Type,
        ]
    );
}
