use super::*;

#[test]
pub(super) fn test_break() {
    assert!(matches!(parse_stmt("break\n"), Stmt::Break));
}
