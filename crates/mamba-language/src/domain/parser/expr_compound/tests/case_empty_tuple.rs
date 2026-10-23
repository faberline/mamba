use super::*;

// --- Parenthesized ---

#[test]
pub(super) fn test_empty_tuple() {
    match parse_expr("()") {
        Expr::TupleLit(elems) => assert!(elems.is_empty()),
        other => panic!("expected TupleLit([]), got {other:?}"),
    }
}
