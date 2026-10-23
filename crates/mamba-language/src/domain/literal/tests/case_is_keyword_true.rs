use super::*;

// --- is_keyword ---

#[test]
pub(super) fn test_is_keyword_true() {
    assert!(TokenKind::Def.is_keyword());
    assert!(TokenKind::Return.is_keyword());
    assert!(TokenKind::If.is_keyword());
    assert!(TokenKind::Try.is_keyword());
    assert!(TokenKind::Async.is_keyword());
    assert!(TokenKind::Lambda.is_keyword());
}
