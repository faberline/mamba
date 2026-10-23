use super::*;

#[test]
pub(super) fn test_for_doubly_nested_paren_target() {
    let module = parse(
        "for (a, b), (c, d) in items:\n\
         \x20   pass\n",
    );
    match &module.stmts[0].node {
        Stmt::For { targets, body, .. } => {
            assert_eq!(targets.len(), 2);
            assert!(targets[0].starts_with("__for_target_"));
            assert!(targets[1].starts_with("__for_target_"));
            assert_eq!(
                body.len(),
                3,
                "expected 2 prepended unpacks + pass body stmt, got {}",
                body.len()
            );
        }
        _ => panic!("expected For"),
    }
}
