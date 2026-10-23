use super::*;

// --- Token creation ---

#[test]
pub(super) fn test_token_new() {
    let tok = Token::new(TokenKind::Def, 0, 3);
    assert_eq!(tok.kind, TokenKind::Def);
    assert_eq!(tok.start, 0);
    assert_eq!(tok.end, 3);
}
