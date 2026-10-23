use super::*;

#[test]
pub(super) fn test_lex_triple_line_continuation() {
    let kinds = lex_kinds("\"\"\"\\\nbody\n\"\"\"");
    assert_eq!(kinds, vec![TokenKind::TripleStr("body\n".into())]);
}
