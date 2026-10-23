use super::*;

#[test]
pub(super) fn test_yield_from() {
    match parse_expr("yield from gen") {
        Expr::YieldFrom(expr) => {
            assert!(matches!(expr.node, Expr::Ident(ref n) if n == "gen"));
        }
        other => panic!("expected YieldFrom, got {other:?}"),
    }
}
