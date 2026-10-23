use super::*;

#[test]
pub(super) fn test_binary_operators() {
    let module = parse("1 + 2 * 3\n");
    assert_eq!(module.stmts.len(), 1);
    match &module.stmts[0].node {
        Stmt::ExprStmt(expr) => {
            // Should be Add(1, Mul(2, 3)) due to precedence
            match &expr.node {
                Expr::BinOp {
                    op: BinOp::Add,
                    lhs,
                    rhs,
                } => {
                    assert!(matches!(&lhs.node, Expr::IntLit(1)));
                    assert!(matches!(&rhs.node, Expr::BinOp { op: BinOp::Mul, .. }));
                }
                _ => panic!("expected BinOp::Add"),
            }
        }
        _ => panic!("expected ExprStmt"),
    }
}
