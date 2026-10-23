use super::*;

// --- Display ---

#[test]
pub(super) fn test_display_keywords() {
    assert_eq!(format!("{}", TokenKind::Def), "def");
    assert_eq!(format!("{}", TokenKind::Return), "return");
    assert_eq!(format!("{}", TokenKind::True), "True");
    assert_eq!(format!("{}", TokenKind::False), "False");
    assert_eq!(format!("{}", TokenKind::None_), "None");
}
