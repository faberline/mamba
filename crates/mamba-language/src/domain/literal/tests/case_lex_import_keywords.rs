use super::*;

#[test]
pub(super) fn test_lex_import_keywords() {
    let kinds = lex_kinds("import from as");
    assert_eq!(
        kinds,
        vec![TokenKind::Import, TokenKind::From, TokenKind::As,]
    );
}
