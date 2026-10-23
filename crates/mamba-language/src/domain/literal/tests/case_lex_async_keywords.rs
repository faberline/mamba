use super::*;

#[test]
pub(super) fn test_lex_async_keywords() {
    let kinds = lex_kinds("async await yield");
    assert_eq!(
        kinds,
        vec![TokenKind::Async, TokenKind::Await, TokenKind::Yield,]
    );
}
