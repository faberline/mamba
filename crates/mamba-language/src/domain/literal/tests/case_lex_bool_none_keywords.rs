use super::*;

#[test]
pub(super) fn test_lex_bool_none_keywords() {
    let kinds = lex_kinds("True False None");
    assert_eq!(
        kinds,
        vec![TokenKind::True, TokenKind::False, TokenKind::None_,]
    );
}
