use super::*;

#[test]
pub(super) fn test_lex_comparison_operators() {
    let kinds = lex_kinds("== != < > <= >=");
    assert_eq!(
        kinds,
        vec![
            TokenKind::EqEq,
            TokenKind::NotEq,
            TokenKind::Lt,
            TokenKind::Gt,
            TokenKind::LtEq,
            TokenKind::GtEq,
        ]
    );
}
