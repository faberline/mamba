use super::*;

#[test]
pub(super) fn test_lex_raw_triple_dquote() {
    // #1678 — `r"""...` previously matched the single-quoted regex `r""`
    // and left the rest of the source as garbage.
    let kinds = lex_kinds("r\"\"\"x = b\"hi\" \"\"\"");
    assert_eq!(kinds, vec![TokenKind::RawStr("x = b\"hi\" ".into())]);
}
