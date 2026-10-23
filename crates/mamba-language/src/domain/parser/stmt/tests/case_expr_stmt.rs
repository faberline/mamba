use super::*;

// --- Expression statement ---

#[test]
pub(super) fn test_expr_stmt() {
    match parse_stmt("42\n") {
        Stmt::ExprStmt(e) => {
            assert!(matches!(e.node, Expr::IntLit(42)));
        }
        other => panic!("expected ExprStmt, got {other:?}"),
    }
}
