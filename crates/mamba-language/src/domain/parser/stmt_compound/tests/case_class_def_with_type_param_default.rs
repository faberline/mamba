use super::*;

#[test]
pub(super) fn test_class_def_with_type_param_default() {
    match parse_stmt("class Foo[T = int]:\n    pass\n") {
        Stmt::ClassDef { type_params, .. } => {
            assert_eq!(type_params.len(), 1);
            assert!(matches!(
                type_params[0].default.as_ref().map(|expr| &expr.node),
                Some(Expr::Ident(name)) if name == "int"
            ));
        }
        other => panic!("expected ClassDef, got {other:?}"),
    }
}
