use super::*;

#[test]
pub(super) fn test_in_and_not_in() {
    match parse_expr_str("x in y") {
        Expr::BinOp { op: BinOp::In, .. } => {}
        other => panic!("expected In, got {other:?}"),
    }
    match parse_expr_str("x not in y") {
        Expr::BinOp {
            op: BinOp::NotIn, ..
        } => {}
        other => panic!("expected NotIn, got {other:?}"),
    }
}
