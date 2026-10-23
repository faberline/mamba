use super::*;

#[test]
pub(super) fn test_is_and_is_not() {
    match parse_expr_str("x is None") {
        Expr::BinOp { op: BinOp::Is, .. } => {}
        other => panic!("expected Is, got {other:?}"),
    }
    match parse_expr_str("x is not None") {
        Expr::BinOp {
            op: BinOp::IsNot, ..
        } => {}
        other => panic!("expected IsNot, got {other:?}"),
    }
}
