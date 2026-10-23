use super::*;

/// Parsing `{1, 2}` as expression statement should produce a SetLit.
#[test]
pub(super) fn test_set_literal_two_elements() {
    match parse_expr_str("{1, 2}") {
        Expr::SetLit(items) => {
            assert_eq!(items.len(), 2);
            assert!(matches!(items[0].node, Expr::IntLit(1)));
            assert!(matches!(items[1].node, Expr::IntLit(2)));
        }
        other => panic!("expected SetLit, got {other:?}"),
    }
}
