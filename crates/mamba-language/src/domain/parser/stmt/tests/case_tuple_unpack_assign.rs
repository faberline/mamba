use super::*;

#[test]
pub(super) fn test_tuple_unpack_assign() {
    match parse_stmt("a, b = 1, 2\n") {
        Stmt::Assign { target, value } => {
            assert!(matches!(target.node, Expr::TupleLit(_)));
            assert!(matches!(value.node, Expr::TupleLit(_)));
        }
        other => panic!("expected Assign(tuple), got {other:?}"),
    }
}
