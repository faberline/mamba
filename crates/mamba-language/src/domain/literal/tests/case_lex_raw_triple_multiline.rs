use super::*;

#[test]
pub(super) fn test_lex_raw_triple_multiline() {
    let kinds = lex_kinds("r\"\"\"line1\nline2\\n\"\"\"");
    assert_eq!(kinds, vec![TokenKind::RawStr("line1\nline2\\n".into())]);
}
