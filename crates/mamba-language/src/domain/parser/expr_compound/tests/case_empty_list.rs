use super::*;

// --- List ---

#[test]
pub(super) fn test_empty_list() {
    match parse_expr("[]") {
        Expr::ListLit(elems) => assert!(elems.is_empty()),
        other => panic!("expected ListLit([]), got {other:?}"),
    }
}
