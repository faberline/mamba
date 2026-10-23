use super::*;

// --- Set trailing comma ---

#[test]
pub(super) fn test_set_trailing_comma() {
    match parse_expr("{1, 2,}") {
        Expr::SetLit(elems) => assert_eq!(elems.len(), 2),
        other => panic!("expected SetLit, got {other:?}"),
    }
}
