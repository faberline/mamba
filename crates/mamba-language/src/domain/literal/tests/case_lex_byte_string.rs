use super::*;

// --- Byte strings ---

#[test]
pub(super) fn test_lex_byte_string() {
    let kinds = lex_kinds("b\"bytes\"");
    assert_eq!(kinds, vec![TokenKind::ByteStr("bytes".into())]);
}
