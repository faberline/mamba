use super::*;

// --- Try / Except ---

#[test]
pub(super) fn test_try_except_basic() {
    match parse_stmt("try:\n    pass\nexcept:\n    pass\n") {
        Stmt::Try {
            body,
            handlers,
            else_body,
            finally_body,
        } => {
            assert_eq!(body.len(), 1);
            assert_eq!(handlers.len(), 1);
            assert!(handlers[0].exc_type.is_none());
            assert!(handlers[0].name.is_none());
            assert!(else_body.is_none());
            assert!(finally_body.is_none());
        }
        other => panic!("expected Try, got {other:?}"),
    }
}
