use super::*;

#[test]
pub(super) fn test_function_def() {
    let module = parse("def add(a: int, b: int) -> int:\n    return a + b\n");
    assert_eq!(module.stmts.len(), 1);
    match &module.stmts[0].node {
        Stmt::FnDef {
            name,
            params,
            return_ty,
            body,
            ..
        } => {
            assert_eq!(name, "add");
            assert_eq!(params.len(), 2);
            assert_eq!(params[0].name, "a");
            assert_eq!(params[1].name, "b");
            assert!(return_ty.is_some());
            assert_eq!(body.len(), 1);
            assert!(matches!(&body[0].node, Stmt::Return(Some(_))));
        }
        _ => panic!("expected FnDef"),
    }
}
