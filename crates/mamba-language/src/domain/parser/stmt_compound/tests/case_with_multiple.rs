use super::*;

#[test]
pub(super) fn test_with_multiple() {
    match parse_stmt("with a as x, b as y:\n    pass\n") {
        Stmt::With { items, .. } => {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].alias.as_deref(), Some("x"));
            assert_eq!(items[1].alias.as_deref(), Some("y"));
        }
        other => panic!("expected With, got {other:?}"),
    }
}
