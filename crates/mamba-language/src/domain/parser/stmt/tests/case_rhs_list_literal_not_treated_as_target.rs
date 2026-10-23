use super::*;

#[test]
pub(super) fn test_rhs_list_literal_not_treated_as_target() {
    // The list literal on the RHS must stay a ListLit (not normalized).
    match parse_stmt("v = [1, 2]\n") {
        Stmt::Assign { value, .. } => {
            assert!(matches!(value.node, Expr::ListLit(_)));
        }
        other => panic!("expected Assign with ListLit value, got {other:?}"),
    }
}
