use super::*;

// --- For ---

#[test]
pub(super) fn test_for_basic() {
    match parse_stmt("for x in items:\n    pass\n") {
        Stmt::For {
            targets,
            iter,
            body,
            else_body,
            ..
        } => {
            assert_eq!(targets, vec!["x"]);
            assert_eq!(body.len(), 1);
            assert!(else_body.is_none());
            assert!(matches!(iter.node, Expr::Ident(ref n) if n == "items"));
        }
        other => panic!("expected For, got {other:?}"),
    }
}
