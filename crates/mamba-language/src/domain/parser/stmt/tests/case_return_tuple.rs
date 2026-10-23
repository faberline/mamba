use super::*;

#[test]
pub(super) fn test_return_tuple() {
    match parse_stmt("return 1, 2\n") {
        Stmt::Return(Some(e)) => {
            assert!(matches!(e.node, Expr::TupleLit(_)));
        }
        other => panic!("expected Return(TupleLit), got {other:?}"),
    }
}
