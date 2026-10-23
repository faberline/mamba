use super::*;

#[test]
pub(super) fn test_unary_not() {
    match parse_expr_str("not True") {
        Expr::UnaryOp {
            op: UnaryOp::Not,
            operand,
        } => {
            assert!(matches!(operand.node, Expr::BoolLit(true)));
        }
        other => panic!("expected UnaryOp::Not, got {other:?}"),
    }
}
