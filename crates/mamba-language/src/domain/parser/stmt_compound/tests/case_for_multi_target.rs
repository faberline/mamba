use super::*;

#[test]
pub(super) fn test_for_multi_target() {
    match parse_stmt("for k, v in items:\n    pass\n") {
        Stmt::For { targets, .. } => {
            assert_eq!(targets, vec!["k", "v"]);
        }
        other => panic!("expected For, got {other:?}"),
    }
}
