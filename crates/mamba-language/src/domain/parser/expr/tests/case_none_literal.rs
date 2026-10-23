use super::*;

#[test]
pub(super) fn test_none_literal() {
    assert!(matches!(parse_expr_str("None"), Expr::NoneLit));
}
