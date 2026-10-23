use super::*;

#[test]
pub(super) fn test_async_for() {
    match parse_stmt("async for x in items:\n    pass\n") {
        Stmt::AsyncFor { targets, .. } => {
            assert_eq!(targets, vec!["x"]);
        }
        other => panic!("expected AsyncFor, got {other:?}"),
    }
}
