use super::*;

#[test]
pub(super) fn test_decorator_on_async_fn() {
    match parse_stmt("@my_dec\nasync def foo():\n    pass\n") {
        Stmt::AsyncFnDef {
            decorators, name, ..
        } => {
            assert_eq!(decorators.len(), 1);
            assert_eq!(name, "foo");
        }
        other => panic!("expected AsyncFnDef, got {other:?}"),
    }
}
