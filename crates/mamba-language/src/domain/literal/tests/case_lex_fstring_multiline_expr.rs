use super::*;

#[test]
pub(super) fn test_lex_fstring_multiline_expr() {
    // f"value: {\n    x + y\n}" — multiline expression
    let src = "f\"value: {\n    x + y\n}\"";
    let kinds = lex_kinds(src);
    assert_eq!(kinds.len(), 1);
    assert!(matches!(&kinds[0], TokenKind::FStr(s) if s.contains("x + y")));
}
