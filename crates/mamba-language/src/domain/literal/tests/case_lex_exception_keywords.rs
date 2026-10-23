use super::*;

#[test]
pub(super) fn test_lex_exception_keywords() {
    let kinds = lex_kinds("try except finally raise with");
    assert_eq!(
        kinds,
        vec![
            TokenKind::Try,
            TokenKind::Except,
            TokenKind::Finally,
            TokenKind::Raise,
            TokenKind::With,
        ]
    );
}
