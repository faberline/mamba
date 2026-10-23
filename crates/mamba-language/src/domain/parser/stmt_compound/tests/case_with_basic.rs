use super::*;

// --- With ---

#[test]
pub(super) fn test_with_basic() {
    match parse_stmt("with open(\"f\") as f:\n    pass\n") {
        Stmt::With { items, body } => {
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].alias.as_deref(), Some("f"));
            assert_eq!(body.len(), 1);
        }
        other => panic!("expected With, got {other:?}"),
    }
}
