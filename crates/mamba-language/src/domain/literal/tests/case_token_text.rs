use super::*;

#[test]
pub(super) fn test_token_text() {
    let source = "def foo():";
    let tok = Token::new(TokenKind::Def, 0, 3);
    assert_eq!(tok.text(source), "def");
}
