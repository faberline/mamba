use super::*;

#[test]
pub(super) fn test_unary_bitnot() {
    match parse_expr_str("~x") {
        Expr::UnaryOp {
            op: UnaryOp::BitNot,
            operand,
        } => {
            assert!(matches!(operand.node, Expr::Ident(ref n) if n == "x"));
        }
        other => panic!("expected UnaryOp::BitNot, got {other:?}"),
    }
}
