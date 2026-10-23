use super::*;

#[test]
pub(super) fn test_lex_fstring_escaped_braces() {
    // f"{{literal}}" — escaped braces should not start an expression
    let kinds = lex_kinds("f\"{{literal}}\"");
    assert_eq!(kinds, vec![TokenKind::FStr("{{literal}}".into())]);
}
