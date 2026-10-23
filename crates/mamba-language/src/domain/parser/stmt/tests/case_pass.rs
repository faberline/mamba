use super::*;

// --- Simple statements ---

#[test]
pub(super) fn test_pass() {
    assert!(matches!(parse_stmt("pass\n"), Stmt::Pass));
}
