use super::*;

// --- Set ---

#[test]
pub(super) fn test_set_literal() {
    match parse_expr("{1, 2, 3}") {
        Expr::SetLit(elems) => assert_eq!(elems.len(), 3),
        other => panic!("expected SetLit, got {other:?}"),
    }
}
