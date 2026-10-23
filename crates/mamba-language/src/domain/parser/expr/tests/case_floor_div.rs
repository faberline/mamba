use super::*;

#[test]
pub(super) fn test_floor_div() {
    match parse_expr_str("a // b") {
        Expr::BinOp {
            op: BinOp::FloorDiv,
            ..
        } => {}
        other => panic!("expected FloorDiv, got {other:?}"),
    }
}
