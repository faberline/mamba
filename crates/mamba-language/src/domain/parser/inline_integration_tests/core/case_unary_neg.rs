use super::*;

#[test]
pub(super) fn test_unary_neg() {
    let module = parse("-42\n");
    match &module.stmts[0].node {
        Stmt::ExprStmt(expr) => {
            assert!(matches!(
                &expr.node,
                Expr::UnaryOp {
                    op: UnaryOp::Neg,
                    ..
                }
            ));
        }
        _ => panic!("expected UnaryOp"),
    }
}
