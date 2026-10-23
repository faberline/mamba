use super::*;

#[test]
pub(super) fn test_display_special() {
    assert_eq!(format!("{}", TokenKind::Ellipsis), "...");
    assert_eq!(format!("{}", TokenKind::Indent), "INDENT");
    assert_eq!(format!("{}", TokenKind::Dedent), "DEDENT");
    assert_eq!(format!("{}", TokenKind::Eof), "EOF");
    assert_eq!(format!("{}", TokenKind::Comment), "comment");
}
