use super::*;

#[test]
pub(super) fn test_logical_and_or() {
    // a or b and c => Or(a, And(b, c))
    match parse_expr_str("a or b and c") {
        Expr::BinOp {
            op: BinOp::Or,
            lhs,
            rhs,
        } => {
            assert!(matches!(lhs.node, Expr::Ident(ref n) if n == "a"));
            assert!(matches!(rhs.node, Expr::BinOp { op: BinOp::And, .. }));
        }
        other => panic!("expected Or(a, And(b,c)), got {other:?}"),
    }
}
