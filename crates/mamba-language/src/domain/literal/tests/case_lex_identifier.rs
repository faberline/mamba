use super::*;

// --- Identifiers ---

#[test]
pub(super) fn test_lex_identifier() {
    let kinds = lex_kinds("my_var");
    assert_eq!(kinds, vec![TokenKind::Ident]);
}
