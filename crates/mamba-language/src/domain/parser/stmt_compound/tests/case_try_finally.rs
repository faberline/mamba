use super::*;

#[test]
pub(super) fn test_try_finally() {
    match parse_stmt("try:\n    pass\nfinally:\n    pass\n") {
        Stmt::Try { finally_body, .. } => {
            assert!(finally_body.is_some());
        }
        other => panic!("expected Try, got {other:?}"),
    }
}
