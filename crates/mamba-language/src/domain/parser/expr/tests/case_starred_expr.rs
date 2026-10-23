use super::*;

// --- Starred ---

#[test]
pub(super) fn test_starred_expr() {
    match parse_expr_str("*a") {
        Expr::Starred(inner) => {
            assert!(matches!(inner.node, Expr::Ident(ref n) if n == "a"));
        }
        other => panic!("expected Starred, got {other:?}"),
    }
}
