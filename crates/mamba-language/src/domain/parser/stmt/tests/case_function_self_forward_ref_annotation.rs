use super::*;

#[test]
pub(super) fn test_function_self_forward_ref_annotation() {
    match parse_stmt("def foo(self: 'ForRefExample'):\n    pass\n") {
        Stmt::FnDef { params, .. } => {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].name, "self");
            assert!(matches!(
                &params[0].ty.node,
                TypeExpr::Named(n) if strip_forward_ref_name(n) == Some("ForRefExample")
            ));
        }
        other => panic!("expected FnDef, got {other:?}"),
    }
}
