use super::*;

// --- Unary operators ---

#[test]
pub(super) fn test_unary_pos() {
    match parse_expr_str("+5") {
        Expr::UnaryOp {
            op: UnaryOp::Pos,
            operand,
        } => {
            assert!(matches!(operand.node, Expr::IntLit(5)));
        }
        other => panic!("expected UnaryOp::Pos, got {other:?}"),
    }
}
