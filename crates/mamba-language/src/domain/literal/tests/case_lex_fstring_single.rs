use super::*;

#[test]
pub(super) fn test_lex_fstring_single() {
    let kinds = lex_kinds("f'value: {x}'");
    assert_eq!(kinds, vec![TokenKind::FStr("value: {x}".into())]);
}
