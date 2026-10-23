use super::*;

// --- Error cases ---

#[test]
pub(super) fn test_unexpected_token_error() {
    let result = parser::parse(")\n", fid());
    assert!(result.is_err());
}
