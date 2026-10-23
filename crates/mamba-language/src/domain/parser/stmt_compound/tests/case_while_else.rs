use super::*;

#[test]
pub(super) fn test_while_else() {
    match parse_stmt("while True:\n    pass\nelse:\n    pass\n") {
        Stmt::While { else_body, .. } => {
            assert!(else_body.is_some());
        }
        other => panic!("expected While, got {other:?}"),
    }
}
