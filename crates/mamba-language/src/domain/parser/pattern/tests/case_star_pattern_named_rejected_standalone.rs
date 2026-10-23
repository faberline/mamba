use super::*;

// --- Star pattern: only valid inside sequence patterns (#827) ---

#[test]
pub(super) fn test_star_pattern_named_rejected_standalone() {
    // `case *rest:` is invalid PEP 634 — star patterns only inside sequences.
    let src = "match x:\n    case *rest:\n        pass\n";
    let result = parser::parse(src, fid());
    assert!(
        result.is_err(),
        "standalone star pattern should be rejected"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("star") || msg.contains("sequence"),
        "error should mention star or sequence, got: {msg}"
    );
}
