use super::*;

// --- Float literals ---

#[test]
pub(super) fn test_lex_float() {
    let kinds = lex_kinds("3.14");
    assert_eq!(
        kinds,
        vec![TokenKind::Float(f64::from_bits(0x4009_1eb8_51eb_851f))]
    );
}
