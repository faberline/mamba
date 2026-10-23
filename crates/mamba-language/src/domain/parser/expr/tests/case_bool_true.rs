use super::*;

#[test]
pub(super) fn test_bool_true() {
    assert!(matches!(parse_expr_str("True"), Expr::BoolLit(true)));
}
