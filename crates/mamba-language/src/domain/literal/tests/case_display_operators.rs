use super::*;

#[test]
pub(super) fn test_display_operators() {
    assert_eq!(format!("{}", TokenKind::Plus), "+");
    assert_eq!(format!("{}", TokenKind::Arrow), "->");
    assert_eq!(format!("{}", TokenKind::EqEq), "==");
    assert_eq!(format!("{}", TokenKind::DoubleStar), "**");
}
