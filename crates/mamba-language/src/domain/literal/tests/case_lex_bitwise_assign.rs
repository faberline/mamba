use super::*;

#[test]
pub(super) fn test_lex_bitwise_assign() {
    let kinds = lex_kinds("&= |= ^= <<= >>=");
    assert_eq!(
        kinds,
        vec![
            TokenKind::AmpEq,
            TokenKind::PipeEq,
            TokenKind::CaretEq,
            TokenKind::LShiftEq,
            TokenKind::RShiftEq,
        ]
    );
}
