use super::*;

// --- Del ---

#[test]
pub(super) fn test_del() {
    match parse_stmt("del x\n") {
        Stmt::Del(e) => {
            assert!(matches!(e.node, Expr::Ident(ref n) if n == "x"));
        }
        other => panic!("expected Del, got {other:?}"),
    }
}
