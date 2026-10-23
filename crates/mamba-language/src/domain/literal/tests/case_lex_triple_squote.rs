use super::*;

#[test]
pub(super) fn test_lex_triple_squote() {
    let kinds = lex_kinds("'''another\nmulti'''");
    assert_eq!(kinds, vec![TokenKind::TripleStr("another\nmulti".into())]);
}
