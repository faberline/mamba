use super::*;

// --- Ternary ---

#[test]
pub(super) fn test_ternary_expr() {
    match parse_expr("a if cond else b") {
        Expr::IfExpr {
            body,
            condition,
            else_body,
        } => {
            assert!(matches!(body.node, Expr::Ident(ref n) if n == "a"));
            assert!(matches!(condition.node, Expr::Ident(ref n) if n == "cond"));
            assert!(matches!(else_body.node, Expr::Ident(ref n) if n == "b"));
        }
        other => panic!("expected IfExpr, got {other:?}"),
    }
}
