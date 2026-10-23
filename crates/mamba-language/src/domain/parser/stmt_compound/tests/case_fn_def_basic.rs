use super::*;

// --- Function def ---

#[test]
pub(super) fn test_fn_def_basic() {
    match parse_stmt("def foo():\n    pass\n") {
        Stmt::FnDef {
            name,
            params,
            return_ty,
            body,
            decorators,
            ..
        } => {
            assert_eq!(name, "foo");
            assert!(params.is_empty());
            assert!(return_ty.is_none());
            assert!(decorators.is_empty());
            assert_eq!(body.len(), 1);
        }
        other => panic!("expected FnDef, got {other:?}"),
    }
}
