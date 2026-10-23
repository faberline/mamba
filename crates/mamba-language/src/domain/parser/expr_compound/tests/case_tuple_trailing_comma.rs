use super::*;

#[test]
pub(super) fn test_tuple_trailing_comma() {
    match parse_expr("(1,)") {
        Expr::TupleLit(elems) => assert_eq!(elems.len(), 1),
        other => panic!("expected TupleLit, got {other:?}"),
    }
}
