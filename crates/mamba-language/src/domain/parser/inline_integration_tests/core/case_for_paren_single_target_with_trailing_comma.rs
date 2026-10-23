use super::*;

#[test]
pub(super) fn test_for_paren_single_target_with_trailing_comma() {
    let module = parse("for (a,) in items:\n    pass\n");
    match &module.stmts[0].node {
        Stmt::For { targets, .. } => {
            assert_eq!(targets, &["a"]);
        }
        _ => panic!("expected For"),
    }
}
