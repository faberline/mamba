use super::*;

// --- Await ---

#[test]
pub(super) fn test_await_expr() {
    match parse_expr("await coro()") {
        Expr::Await(inner) => {
            assert!(matches!(inner.node, Expr::Call { .. }));
        }
        other => panic!("expected Await, got {other:?}"),
    }
}
