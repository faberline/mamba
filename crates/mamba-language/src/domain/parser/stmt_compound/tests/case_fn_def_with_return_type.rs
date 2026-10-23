use super::*;

#[test]
pub(super) fn test_fn_def_with_return_type() {
    match parse_stmt("def foo() -> int:\n    return 0\n") {
        Stmt::FnDef { return_ty, .. } => {
            let rt = return_ty.unwrap();
            assert!(matches!(rt.node, TypeExpr::Named(ref n) if n == "int"));
        }
        other => panic!("expected FnDef, got {other:?}"),
    }
}
