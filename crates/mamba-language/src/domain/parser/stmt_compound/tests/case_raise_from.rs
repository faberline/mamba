use super::*;

#[test]
pub(super) fn test_raise_from() {
    match parse_stmt("raise ValueError() from e\n") {
        Stmt::Raise {
            value: Some(_),
            from: Some(f),
        } => {
            assert!(matches!(f.node, Expr::Ident(ref n) if n == "e"));
        }
        other => panic!("expected Raise from, got {other:?}"),
    }
}
