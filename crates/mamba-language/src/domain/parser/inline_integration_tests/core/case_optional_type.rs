use super::*;

#[test]
pub(super) fn test_optional_type() {
    let module = parse("x: int? = None\n");
    assert_eq!(module.stmts.len(), 1);
    match &module.stmts[0].node {
        Stmt::VarDecl { ty, .. } => {
            assert!(matches!(&ty.node, TypeExpr::Optional(_)));
        }
        _ => panic!("expected VarDecl with optional type"),
    }
}
