use super::*;

#[test]
pub(super) fn test_pow_right_associative() {
    // 2 ** 3 ** 4 => Pow(2, Pow(3, 4))
    match parse_expr_str("2 ** 3 ** 4") {
        Expr::BinOp {
            op: BinOp::Pow,
            lhs,
            rhs,
        } => {
            assert!(matches!(lhs.node, Expr::IntLit(2)));
            match rhs.node {
                Expr::BinOp {
                    op: BinOp::Pow,
                    lhs: inner_l,
                    rhs: inner_r,
                } => {
                    assert!(matches!(inner_l.node, Expr::IntLit(3)));
                    assert!(matches!(inner_r.node, Expr::IntLit(4)));
                }
                other => panic!("expected inner Pow, got {other:?}"),
            }
        }
        other => panic!("expected Pow, got {other:?}"),
    }
}
