use super::*;

#[test]
pub(super) fn test_for_else() {
    match parse_stmt("for x in items:\n    pass\nelse:\n    pass\n") {
        Stmt::For { else_body, .. } => {
            assert!(else_body.is_some());
        }
        other => panic!("expected For, got {other:?}"),
    }
}
