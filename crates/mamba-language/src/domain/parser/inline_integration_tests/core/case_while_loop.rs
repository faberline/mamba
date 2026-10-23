use super::*;

#[test]
pub(super) fn test_while_loop() {
    let src = "while i < 10:\n    i = i + 1\n";
    let module = parse(src);
    assert_eq!(module.stmts.len(), 1);
    match &module.stmts[0].node {
        Stmt::While { body, .. } => {
            assert_eq!(body.len(), 1);
        }
        _ => panic!("expected While"),
    }
}
