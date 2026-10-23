use super::*;

#[test]
pub(super) fn test_is_keyword_false() {
    assert!(!TokenKind::Plus.is_keyword());
    assert!(!TokenKind::Ident.is_keyword());
    assert!(!TokenKind::Int(42).is_keyword());
    assert!(!TokenKind::LParen.is_keyword());
    assert!(!TokenKind::Comment.is_keyword());
    assert!(!TokenKind::Newline.is_keyword());
}
