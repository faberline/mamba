use super::*;

// --- Async fn def ---

#[test]
pub(super) fn test_async_fn_def() {
    match parse_stmt("async def foo():\n    pass\n") {
        Stmt::AsyncFnDef { name, .. } => assert_eq!(name, "foo"),
        other => panic!("expected AsyncFnDef, got {other:?}"),
    }
}
