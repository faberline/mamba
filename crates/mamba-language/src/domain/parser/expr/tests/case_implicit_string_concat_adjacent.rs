use super::*;

#[test]
pub(super) fn test_implicit_string_concat_adjacent() {
    // PEP 3105 — adjacent string literals concatenate at compile time.
    match parse_expr_str("\"foo\" \"bar\"") {
        Expr::StrLit(s) => assert_eq!(s, "foobar"),
        other => panic!("expected StrLit, got {other:?}"),
    }
}
