use super::*;

#[test]
pub(super) fn test_if_elif_else() {
    let src = "if x:\n    pass\nelif y:\n    pass\nelif z:\n    pass\nelse:\n    pass\n";
    match parse_stmt(src) {
        Stmt::If {
            elif_clauses,
            else_body,
            ..
        } => {
            assert_eq!(elif_clauses.len(), 2);
            assert!(else_body.is_some());
        }
        other => panic!("expected If, got {other:?}"),
    }
}
