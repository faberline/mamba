use super::*;

// --- LBrace-leading statements (#1112) ---

#[test]
pub(super) fn test_bare_dict_expr_stmt() {
    // Bare dict literal as expression statement
    match parse_stmt("{}\n") {
        Stmt::ExprStmt(e) => {
            assert!(matches!(e.node, Expr::DictLit(ref entries) if entries.is_empty()));
        }
        other => panic!("expected ExprStmt(DictLit), got {other:?}"),
    }
}
