use super::*;

// --- Keywords ---

#[test]
pub(super) fn test_lex_all_control_flow_keywords() {
    let kinds = lex_kinds("def return if elif else while for in");
    assert_eq!(
        kinds,
        vec![
            TokenKind::Def,
            TokenKind::Return,
            TokenKind::If,
            TokenKind::Elif,
            TokenKind::Else,
            TokenKind::While,
            TokenKind::For,
            TokenKind::In,
        ]
    );
}
