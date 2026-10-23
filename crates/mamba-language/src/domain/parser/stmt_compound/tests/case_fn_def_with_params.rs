use super::*;

#[test]
pub(super) fn test_fn_def_with_params() {
    match parse_stmt("def foo(a: int, b: str):\n    pass\n") {
        Stmt::FnDef { params, .. } => {
            assert_eq!(params.len(), 2);
            assert_eq!(params[0].name, "a");
            assert_eq!(params[1].name, "b");
        }
        other => panic!("expected FnDef, got {other:?}"),
    }
}
