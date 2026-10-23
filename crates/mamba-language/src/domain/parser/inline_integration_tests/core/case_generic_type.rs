use super::*;

#[test]
pub(super) fn test_generic_type() {
    let module = parse("x: list[int] = [1]\n");
    assert_eq!(module.stmts.len(), 1);
    match &module.stmts[0].node {
        Stmt::VarDecl { ty, .. } => match &ty.node {
            TypeExpr::Generic { name, args } => {
                assert_eq!(name, "list");
                assert_eq!(args.len(), 1);
            }
            _ => panic!("expected Generic type"),
        },
        _ => panic!("expected VarDecl"),
    }
}
