use super::*;

#[test]
pub(super) fn test_bare_set_expr_stmt() {
    // Bare set literal as expression statement
    match parse_stmt("{1, 2, 3}\n") {
        Stmt::ExprStmt(e) => {
            assert!(matches!(e.node, Expr::SetLit(ref elems) if elems.len() == 3));
        }
        other => panic!("expected ExprStmt(SetLit), got {other:?}"),
    }
}
