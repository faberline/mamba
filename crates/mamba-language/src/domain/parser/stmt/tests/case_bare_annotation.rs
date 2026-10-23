use super::*;

// --- Var decl & bare annotations ---

#[test]
pub(super) fn test_bare_annotation() {
    match parse_stmt("id: int\n") {
        Stmt::BareAnnotation { name, ty } => {
            assert_eq!(name, "id");
            assert!(matches!(ty.node, TypeExpr::Named(ref n) if n == "int"));
        }
        other => panic!("expected BareAnnotation, got {other:?}"),
    }
}
