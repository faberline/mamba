use super::*;

#[test]
pub(super) fn test_list_trailing_comma() {
    match parse_expr("[1, 2,]") {
        Expr::ListLit(elems) => assert_eq!(elems.len(), 2),
        other => panic!("expected ListLit, got {other:?}"),
    }
}
