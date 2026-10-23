use super::*;

#[test]
pub(super) fn test_negative_literal_pattern() {
    match parse_pattern("-1") {
        Pattern::Literal(Expr::UnaryOp {
            op: UnaryOp::Neg,
            operand,
        }) => {
            assert!(matches!(operand.node, Expr::IntLit(1)));
        }
        other => panic!("expected Literal(-1), got {other:?}"),
    }
}
