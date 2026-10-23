use super::*;

#[test]
pub(super) fn test_var_decl() {
    match parse_stmt("x: int = 0\n") {
        Stmt::VarDecl { name, ty, value } => {
            assert_eq!(name, "x");
            assert!(matches!(ty.node, TypeExpr::Named(ref n) if n == "int"));
            assert!(matches!(value.node, Expr::IntLit(0)));
        }
        other => panic!("expected VarDecl, got {other:?}"),
    }
}
