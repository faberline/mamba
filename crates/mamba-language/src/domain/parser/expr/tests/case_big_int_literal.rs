use super::*;

#[test]
pub(super) fn test_big_int_literal() {
    assert!(matches!(
        parse_expr_str("123456789012345678901234567890"),
        Expr::BigIntLit(s) if s == "123456789012345678901234567890"
    ));
}
