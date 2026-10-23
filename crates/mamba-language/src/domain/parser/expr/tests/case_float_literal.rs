use super::*;

#[test]
pub(super) fn test_float_literal() {
    match parse_expr_str("3.14") {
        Expr::FloatLit(v) => assert!((v - f64::from_bits(0x4009_1eb8_51eb_851f)).abs() < 1e-10),
        other => panic!("expected FloatLit, got {other:?}"),
    }
}
