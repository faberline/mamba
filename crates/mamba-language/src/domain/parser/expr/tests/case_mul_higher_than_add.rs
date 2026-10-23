use super::*;

#[test]
pub(super) fn test_mul_higher_than_add() {
    // 1 + 2 * 3 => Add(1, Mul(2, 3))
    match parse_expr_str("1 + 2 * 3") {
        Expr::BinOp {
            op: BinOp::Add,
            lhs,
            rhs,
        } => {
            assert!(matches!(lhs.node, Expr::IntLit(1)));
            assert!(matches!(rhs.node, Expr::BinOp { op: BinOp::Mul, .. }));
        }
        other => panic!("expected Add(1, Mul(2,3)), got {other:?}"),
    }
}
