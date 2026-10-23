use super::*;

#[test]
pub(super) fn test_with_no_alias() {
    match parse_stmt("with lock:\n    pass\n") {
        Stmt::With { items, .. } => {
            assert!(items[0].alias.is_none());
        }
        other => panic!("expected With, got {other:?}"),
    }
}
