use super::*;

// --- apply_escape_sequences ---

#[test]
pub(super) fn test_apply_escape_backslash() {
    // Input: "\\" (2 chars: backslash + backslash) → output: "\" (1 backslash)
    assert_eq!(apply_escape_sequences("\\\\"), "\\");
}
