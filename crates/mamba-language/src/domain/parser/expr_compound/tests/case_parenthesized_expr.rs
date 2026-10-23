use super::*;

#[test]
pub(super) fn test_parenthesized_expr() {
    // `(42)` should unwrap to 42
    match parse_expr("(42)") {
        Expr::IntLit(42) => {}
        other => panic!("expected IntLit(42), got {other:?}"),
    }
}
