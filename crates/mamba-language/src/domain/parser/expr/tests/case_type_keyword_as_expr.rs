use super::*;

// --- Type keywords as expressions ---

#[test]
pub(super) fn test_type_keyword_as_expr() {
    // `int` used as expression (e.g., int(x))
    match parse_expr_str("int") {
        Expr::Ident(n) => assert_eq!(n, "int"),
        other => panic!("expected Ident('int'), got {other:?}"),
    }
}
