use super::*;

#[test]
pub(super) fn test_async_with() {
    match parse_stmt("async with lock:\n    pass\n") {
        Stmt::AsyncWith { items, .. } => {
            assert_eq!(items.len(), 1);
        }
        other => panic!("expected AsyncWith, got {other:?}"),
    }
}
