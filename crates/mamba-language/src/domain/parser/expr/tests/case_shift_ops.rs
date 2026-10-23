use super::*;

#[test]
pub(super) fn test_shift_ops() {
    match parse_expr_str("a << b") {
        Expr::BinOp {
            op: BinOp::LShift, ..
        } => {}
        other => panic!("expected LShift, got {other:?}"),
    }
    match parse_expr_str("a >> b") {
        Expr::BinOp {
            op: BinOp::RShift, ..
        } => {}
        other => panic!("expected RShift, got {other:?}"),
    }
}
