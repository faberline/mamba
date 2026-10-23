use super::*;

#[test]
pub(super) fn test_string_literal() {
    match parse_expr_str("\"hello\"") {
        Expr::StrLit(s) => assert_eq!(s, "hello"),
        other => panic!("expected StrLit, got {other:?}"),
    }
}
