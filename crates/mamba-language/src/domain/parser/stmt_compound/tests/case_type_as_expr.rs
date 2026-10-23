use super::*;

// --- type used as expression (not alias) ---

#[test]
pub(super) fn test_type_as_expr() {
    match parse_stmt("type(x)\n") {
        Stmt::ExprStmt(e) => {
            assert!(matches!(e.node, Expr::Call { .. }));
        }
        other => panic!("expected ExprStmt(Call), got {other:?}"),
    }
}
