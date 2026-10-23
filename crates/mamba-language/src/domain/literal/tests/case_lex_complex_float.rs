use super::*;

#[test]
pub(super) fn test_lex_complex_float() {
    let kinds = lex_kinds("3.14j");
    assert_eq!(
        kinds,
        vec![TokenKind::Complex(f64::from_bits(0x4009_1eb8_51eb_851f))]
    );
}
