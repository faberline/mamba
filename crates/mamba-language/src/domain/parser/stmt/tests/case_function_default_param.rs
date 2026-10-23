use super::*;

#[test]
pub(super) fn test_function_default_param() {
    match parse_stmt("def foo(x: int = 0):\n    pass\n") {
        Stmt::FnDef { params, .. } => {
            assert_eq!(params.len(), 1);
            assert!(params[0].default.is_some());
        }
        other => panic!("expected FnDef, got {other:?}"),
    }
}
