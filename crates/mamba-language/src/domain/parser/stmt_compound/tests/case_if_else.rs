use super::*;

#[test]
pub(super) fn test_if_else() {
    match parse_stmt("if True:\n    pass\nelse:\n    pass\n") {
        Stmt::If { else_body, .. } => {
            assert!(else_body.is_some());
        }
        other => panic!("expected If, got {other:?}"),
    }
}
