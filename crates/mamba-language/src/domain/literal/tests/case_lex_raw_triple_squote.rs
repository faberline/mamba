use super::*;

#[test]
pub(super) fn test_lex_raw_triple_squote() {
    // Backslashes pass through literally — no escape processing.
    let kinds = lex_kinds("r'''multi\\nline\\t'''");
    assert_eq!(kinds, vec![TokenKind::RawStr("multi\\nline\\t".into())]);
}
