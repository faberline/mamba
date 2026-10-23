use super::*;

#[test]
pub(super) fn test_expr_stmt_call() {
    match parse_stmt("print(42)\n") {
        Stmt::ExprStmt(e) => {
            assert!(matches!(e.node, Expr::Call { .. }));
        }
        other => panic!("expected ExprStmt(Call), got {other:?}"),
    }
}
