use super::*;

#[test]
pub(super) fn test_ellipsis() {
    assert!(matches!(parse_expr_str("..."), Expr::Ellipsis));
}
