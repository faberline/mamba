use super::*;

#[test]
pub(super) fn test_lex_walrus_and_at() {
    let kinds = lex_kinds(":= @ @=");
    assert_eq!(
        kinds,
        vec![TokenKind::ColonEq, TokenKind::At, TokenKind::AtEq,]
    );
}
