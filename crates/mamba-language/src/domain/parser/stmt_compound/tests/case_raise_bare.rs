use super::*;

// --- Raise ---

#[test]
pub(super) fn test_raise_bare() {
    match parse_stmt("raise\n") {
        Stmt::Raise {
            value: None,
            from: None,
        } => {}
        other => panic!("expected Raise(None), got {other:?}"),
    }
}
