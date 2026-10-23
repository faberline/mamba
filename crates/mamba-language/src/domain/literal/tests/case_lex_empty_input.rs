use super::*;

#[test]
pub(super) fn test_lex_empty_input() {
    let kinds = lex_kinds("");
    assert!(kinds.is_empty());
}
