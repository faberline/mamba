use super::*;

#[test]
pub(super) fn test_display_literals() {
    assert_eq!(format!("{}", TokenKind::Int(42)), "42");
    assert_eq!(
        format!(
            "{}",
            TokenKind::Float(f64::from_bits(0x4009_1eb8_51eb_851f))
        ),
        "3.14"
    );
    assert_eq!(format!("{}", TokenKind::Str("hi".into())), "\"hi\"");
    assert_eq!(format!("{}", TokenKind::Complex(1.0)), "1j");
}
