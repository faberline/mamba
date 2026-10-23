use super::*;

// --- F-strings ---

#[test]
pub(super) fn test_lex_fstring_double() {
    let kinds = lex_kinds("f\"hello {name}\"");
    assert_eq!(kinds, vec![TokenKind::FStr("hello {name}".into())]);
}
