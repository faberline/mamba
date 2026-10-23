use super::*;

#[test]
pub(super) fn test_unary_neg() {
    match parse_expr_str("-10") {
        Expr::UnaryOp {
            op: UnaryOp::Neg,
            operand,
        } => {
            assert!(matches!(operand.node, Expr::IntLit(10)));
        }
        other => panic!("expected UnaryOp::Neg, got {other:?}"),
    }
}
