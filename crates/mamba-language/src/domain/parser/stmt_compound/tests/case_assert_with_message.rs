use super::*;

#[test]
pub(super) fn test_assert_with_message() {
    match parse_stmt("assert x, \"failed\"\n") {
        Stmt::Assert { msg, .. } => {
            assert!(msg.is_some());
        }
        other => panic!("expected Assert, got {other:?}"),
    }
}
