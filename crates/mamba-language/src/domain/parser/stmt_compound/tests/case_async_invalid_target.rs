use super::*;

// --- Async invalid ---

#[test]
pub(super) fn test_async_invalid_target() {
    let result = parser::parse("async pass\n", fid());
    assert!(result.is_err());
}
