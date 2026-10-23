use super::*;

#[test]
pub(super) fn test_function_call() {
    let module = parse("print(42)\n");
    assert_eq!(module.stmts.len(), 1);
    match &module.stmts[0].node {
        Stmt::ExprStmt(expr) => match &expr.node {
            Expr::Call { func, args } => {
                assert!(matches!(&func.node, Expr::Ident(n) if n == "print"));
                assert_eq!(args.len(), 1);
            }
            _ => panic!("expected Call"),
        },
        _ => panic!("expected ExprStmt"),
    }
}
