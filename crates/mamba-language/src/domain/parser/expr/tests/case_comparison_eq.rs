use super::*;

#[test]
pub(super) fn test_comparison_eq() {
    match parse_expr_str("a == b") {
        Expr::BinOp { op: BinOp::Eq, .. } => {}
        other => panic!("expected BinOp::Eq, got {other:?}"),
    }
}
