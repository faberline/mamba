use super::*;

#[test]
pub(super) fn test_lex_pipe_and_question() {
    let kinds = lex_kinds("| ?");
    assert_eq!(kinds, vec![TokenKind::Pipe, TokenKind::Question]);
}
