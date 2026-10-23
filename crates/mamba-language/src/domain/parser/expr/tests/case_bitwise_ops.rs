use super::*;

#[test]
pub(super) fn test_bitwise_ops() {
    match parse_expr_str("a | b") {
        Expr::BinOp {
            op: BinOp::BitOr, ..
        } => {}
        other => panic!("expected BitOr, got {other:?}"),
    }
    match parse_expr_str("a & b") {
        Expr::BinOp {
            op: BinOp::BitAnd, ..
        } => {}
        other => panic!("expected BitAnd, got {other:?}"),
    }
    match parse_expr_str("a ^ b") {
        Expr::BinOp {
            op: BinOp::BitXor, ..
        } => {}
        other => panic!("expected BitXor, got {other:?}"),
    }
}
