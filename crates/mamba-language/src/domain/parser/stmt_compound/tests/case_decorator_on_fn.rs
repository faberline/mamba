use super::*;

// --- Decorators ---

#[test]
pub(super) fn test_decorator_on_fn() {
    match parse_stmt("@my_dec\ndef foo():\n    pass\n") {
        Stmt::FnDef {
            decorators, name, ..
        } => {
            assert_eq!(decorators.len(), 1);
            assert_eq!(name, "foo");
        }
        other => panic!("expected FnDef, got {other:?}"),
    }
}
