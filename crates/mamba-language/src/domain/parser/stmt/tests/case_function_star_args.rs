use super::*;

#[test]
pub(super) fn test_function_star_args() {
    match parse_stmt("def foo(*args: int):\n    pass\n") {
        Stmt::FnDef { params, .. } => {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].name, "args");
            assert_eq!(params[0].kind, ParamKind::Star);
        }
        other => panic!("expected FnDef, got {other:?}"),
    }
}
