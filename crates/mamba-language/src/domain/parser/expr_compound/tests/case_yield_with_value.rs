use super::*;

#[test]
pub(super) fn test_yield_with_value() {
    match parse_expr("yield 42") {
        Expr::Yield(Some(val)) => {
            assert!(matches!(val.node, Expr::IntLit(42)));
        }
        other => panic!("expected Yield(Some), got {other:?}"),
    }
}
