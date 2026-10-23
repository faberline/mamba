use super::*;

#[test]
pub(super) fn test_ternary_with_complex_cond() {
    match parse_expr("x if a > 0 else y") {
        Expr::IfExpr { condition, .. } => {
            assert!(matches!(condition.node, Expr::BinOp { op: BinOp::Gt, .. }));
        }
        other => panic!("expected IfExpr, got {other:?}"),
    }
}
