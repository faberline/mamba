use super::*;

// --- Decorator error ---

#[test]
pub(super) fn test_decorator_on_invalid_target() {
    let result = parser::parse("@dec\npass\n", fid());
    assert!(result.is_err());
}
