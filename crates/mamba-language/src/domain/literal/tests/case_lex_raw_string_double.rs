use super::*;

// --- Raw strings ---

#[test]
pub(super) fn test_lex_raw_string_double() {
    let kinds = lex_kinds("r\"\\n\\t\"");
    assert_eq!(kinds, vec![TokenKind::RawStr("\\n\\t".into())]);
}
