use super::*;

// --- Dict ---

#[test]
pub(super) fn test_empty_dict() {
    match parse_expr("{}") {
        Expr::DictLit(pairs) => assert!(pairs.is_empty()),
        other => panic!("expected DictLit([]), got {other:?}"),
    }
}
