use super::*;

#[test]
pub(super) fn test_lex_triple_empty() {
    let kinds = lex_kinds("\"\"\"\"\"\"");
    assert_eq!(kinds, vec![TokenKind::TripleStr("".into())]);
}
