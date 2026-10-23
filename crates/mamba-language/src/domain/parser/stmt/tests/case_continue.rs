use super::*;

#[test]
pub(super) fn test_continue() {
    assert!(matches!(parse_stmt("continue\n"), Stmt::Continue));
}
