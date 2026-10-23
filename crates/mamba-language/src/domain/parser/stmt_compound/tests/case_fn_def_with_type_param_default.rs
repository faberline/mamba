use super::*;

#[test]
pub(super) fn test_fn_def_with_type_param_default() {
    match parse_stmt("def foo[T = int]():\n    pass\n") {
        Stmt::FnDef { type_params, .. } => {
            assert_eq!(type_params.len(), 1);
            assert!(matches!(
                type_params[0].default.as_ref().map(|expr| &expr.node),
                Some(Expr::Ident(name)) if name == "int"
            ));
        }
        other => panic!("expected FnDef, got {other:?}"),
    }
}
