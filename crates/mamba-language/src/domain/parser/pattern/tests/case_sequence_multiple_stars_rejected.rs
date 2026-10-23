use super::*;

// --- Parser rejection tests for invalid pattern forms ---

#[test]
pub(super) fn test_sequence_multiple_stars_rejected() {
    // PEP 634: at most one starred element allowed in a sequence pattern
    let src = "match x:\n    case [*a, *b]:\n        pass\n";
    let result = parser::parse(src, fid());
    assert!(
        result.is_err(),
        "multiple starred elements should produce a parse error"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("starred") || err_msg.contains("star"),
        "error should mention starred element, got: {err_msg}"
    );
}
