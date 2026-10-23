use super::*;

#[test]
pub(super) fn test_for_loop() {
    let src = "for i in items:\n    pass\n";
    let module = parse(src);
    match &module.stmts[0].node {
        Stmt::For { targets, .. } => {
            assert_eq!(targets, &["i"]);
        }
        _ => panic!("expected For"),
    }
}
