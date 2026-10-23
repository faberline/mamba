use super::*;

// --- Dict trailing comma ---

#[test]
pub(super) fn test_dict_trailing_comma() {
    match parse_expr("{1: 2,}") {
        Expr::DictLit(pairs) => assert_eq!(pairs.len(), 1),
        other => panic!("expected DictLit, got {other:?}"),
    }
}
