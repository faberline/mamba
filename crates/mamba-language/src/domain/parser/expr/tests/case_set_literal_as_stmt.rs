use super::*;

/// Parsing `{1, 2, 3}` as an expression statement should produce a SetLit.
#[test]
pub(super) fn test_set_literal_as_stmt() {
    match parse_expr_str("{1, 2, 3}") {
        Expr::SetLit(items) => {
            assert_eq!(items.len(), 3, "set literal should have 3 items");
        }
        other => panic!("expected SetLit, got {other:?}"),
    }
}
