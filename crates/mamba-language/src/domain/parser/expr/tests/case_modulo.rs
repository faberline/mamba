use super::*;

#[test]
pub(super) fn test_modulo() {
    match parse_expr_str("a % b") {
        Expr::BinOp { op: BinOp::Mod, .. } => {}
        other => panic!("expected Mod, got {other:?}"),
    }
}
