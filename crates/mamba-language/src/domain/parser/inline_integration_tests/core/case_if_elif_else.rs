use super::*;

#[test]
pub(super) fn test_if_elif_else() {
    let src = "if x > 0:\n    pass\nelif x == 0:\n    pass\nelse:\n    pass\n";
    let module = parse(src);
    assert_eq!(module.stmts.len(), 1);
    match &module.stmts[0].node {
        Stmt::If {
            elif_clauses,
            else_body,
            ..
        } => {
            assert_eq!(elif_clauses.len(), 1);
            assert!(else_body.is_some());
        }
        _ => panic!("expected If"),
    }
}
