use super::*;

// --- Mapping pattern: `**_` is rejected (PEP 634) ---

#[test]
pub(super) fn test_mapping_pattern_double_star_underscore_rejected() {
    // PEP 634 disallows `**_` in mapping patterns; only a real identifier is valid
    let src = "match x:\n    case {**_}:\n        pass\n";
    let result = parser::parse(src, fid());
    assert!(
        result.is_err(),
        "`**_` in mapping pattern should produce a parse error"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("**_") || err_msg.contains("identifier"),
        "error should mention `**_` or identifier, got: {err_msg}"
    );
}
