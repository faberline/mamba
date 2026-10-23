use super::*;

// --- Global / Nonlocal ---

#[test]
pub(super) fn test_global() {
    match parse_stmt("global x, y\n") {
        Stmt::Global(names) => {
            assert_eq!(names, vec!["x", "y"]);
        }
        other => panic!("expected Global, got {other:?}"),
    }
}
