use super::*;

#[test]
pub(super) fn test_lex_flow_keywords() {
    let kinds = lex_kinds("pass break continue self");
    assert_eq!(
        kinds,
        vec![
            TokenKind::Pass,
            TokenKind::Break,
            TokenKind::Continue,
            TokenKind::Self_,
        ]
    );
}
