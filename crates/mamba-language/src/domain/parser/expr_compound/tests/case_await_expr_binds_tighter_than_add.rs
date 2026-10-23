use super::*;

#[test]
pub(super) fn test_await_expr_binds_tighter_than_add() {
    match parse_expr("await left() + await right()") {
        Expr::BinOp {
            op: BinOp::Add,
            lhs,
            rhs,
        } => {
            assert!(matches!(lhs.node, Expr::Await(_)));
            assert!(matches!(rhs.node, Expr::Await(_)));
        }
        other => panic!("expected Add of Await operands, got {other:?}"),
    }
}
