use super::*;

#[test]
pub(super) fn test_var_decl() {
    let module = parse("x: int = 42\n");
    assert_eq!(module.stmts.len(), 1);
    match &module.stmts[0].node {
        Stmt::VarDecl { name, ty, value } => {
            assert_eq!(name, "x");
            assert!(matches!(&ty.node, TypeExpr::Named(n) if n == "int"));
            assert!(matches!(&value.node, Expr::IntLit(42)));
        }
        _ => panic!("expected VarDecl"),
    }
}
