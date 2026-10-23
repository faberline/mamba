use super::*;

// --- Comments ---

#[test]
pub(super) fn test_lex_comment() {
    let kinds = lex_kinds("# this is a comment");
    assert_eq!(kinds, vec![TokenKind::Comment]);
}
