use super::*;

#[test]
pub(super) fn test_lex_raw_string_single() {
    let kinds = lex_kinds("r'path\\to\\file'");
    assert_eq!(kinds, vec![TokenKind::RawStr("path\\to\\file".into())]);
}
