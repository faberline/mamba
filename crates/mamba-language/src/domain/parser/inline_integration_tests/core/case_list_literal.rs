use super::*;

#[test]
pub(super) fn test_list_literal() {
    let module = parse("[1, 2, 3]\n");
    assert_eq!(module.stmts.len(), 1);
    match &module.stmts[0].node {
        Stmt::ExprStmt(expr) => match &expr.node {
            Expr::ListLit(elems) => assert_eq!(elems.len(), 3),
            _ => panic!("expected ListLit"),
        },
        _ => panic!("expected ExprStmt"),
    }
}
