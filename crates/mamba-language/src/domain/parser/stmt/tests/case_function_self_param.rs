use super::*;

// --- Params ---

#[test]
pub(super) fn test_function_self_param() {
    match parse_stmt("def foo(self):\n    pass\n") {
        Stmt::FnDef { params, .. } => {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].name, "self");
            assert_eq!(params[0].kind, ParamKind::Regular);
        }
        other => panic!("expected FnDef, got {other:?}"),
    }
}
