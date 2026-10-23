use super::*;

#[test]
pub(super) fn test_token_clone() {
    let tok = Token::new(TokenKind::Int(42), 0, 2);
    let cloned = tok.clone();
    assert_eq!(tok, cloned);
}
