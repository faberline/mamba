use super::*;

#[test]
pub(super) fn test_bool_false() {
    assert!(matches!(parse_expr_str("False"), Expr::BoolLit(false)));
}
