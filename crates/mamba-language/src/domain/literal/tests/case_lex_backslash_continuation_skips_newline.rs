use super::*;

// --- Backslash line continuation (PEP 8 / Lib-style explicit join) ---

#[test]
pub(super) fn test_lex_backslash_continuation_skips_newline() {
    // `a \<NL>b` should lex as two idents joined into one logical line
    // (no Newline token between them); the trailing whitespace after \
    // is also consumed.
    let kinds = lex_kinds("a \\\n    b");
    assert_eq!(kinds, vec![TokenKind::Ident, TokenKind::Ident]);
}
