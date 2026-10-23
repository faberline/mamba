use super::*;

#[test]
pub(super) fn test_type_alias_with_type_param_default() {
    match parse_stmt("type Container[T = int] = list[T]\n") {
        Stmt::TypeAlias {
            name, type_params, ..
        } => {
            assert_eq!(name, "Container");
            assert_eq!(type_params.len(), 1);
            assert!(matches!(
                type_params[0].default.as_ref().map(|expr| &expr.node),
                Some(Expr::Ident(name)) if name == "int"
            ));
        }
        other => panic!("expected TypeAlias, got {other:?}"),
    }
}
