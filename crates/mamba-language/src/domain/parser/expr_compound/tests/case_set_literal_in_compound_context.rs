use super::*;

/// Parse `{1, 2, 3}` as a set literal.
#[test]
pub(super) fn test_set_literal_in_compound_context() {
    match parse_expr("{1, 2, 3}") {
        Expr::SetLit(items) => {
            assert_eq!(items.len(), 3);
            assert!(matches!(items[0].node, Expr::IntLit(1)));
            assert!(matches!(items[1].node, Expr::IntLit(2)));
            assert!(matches!(items[2].node, Expr::IntLit(3)));
        }
        other => panic!("expected SetLit, got {other:?}"),
    }
}
