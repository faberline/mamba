use super::*;

#[test]
pub(super) fn test_tuple_two_elements() {
    match parse_expr("(1, 2)") {
        Expr::TupleLit(elems) => {
            assert_eq!(elems.len(), 2);
            assert!(matches!(elems[0].node, Expr::IntLit(1)));
            assert!(matches!(elems[1].node, Expr::IntLit(2)));
        }
        other => panic!("expected TupleLit, got {other:?}"),
    }
}
