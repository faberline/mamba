use super::*;

// --- Literals ---

#[test]
pub(super) fn test_int_literal() {
    assert!(matches!(parse_expr_str("42"), Expr::IntLit(42)));
}
