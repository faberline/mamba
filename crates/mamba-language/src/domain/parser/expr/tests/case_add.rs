use super::*;

// --- Binary operators and precedence ---

#[test]
pub(super) fn test_add() {
    match parse_expr_str("1 + 2") {
        Expr::BinOp {
            op: BinOp::Add,
            lhs,
            rhs,
        } => {
            assert!(matches!(lhs.node, Expr::IntLit(1)));
            assert!(matches!(rhs.node, Expr::IntLit(2)));
        }
        other => panic!("expected BinOp::Add, got {other:?}"),
    }
}
