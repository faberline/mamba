use super::*;

// --- Self ---

#[test]
pub(super) fn test_self_expr() {
    match parse_expr_str("self") {
        Expr::Ident(n) => assert_eq!(n, "self"),
        other => panic!("expected Ident('self'), got {other:?}"),
    }
}
