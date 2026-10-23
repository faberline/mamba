use super::*;

#[test]
pub(super) fn test_class_pattern_star_inside_rejected() {
    // PEP 634: starred patterns are not allowed in class patterns
    let src = "match x:\n    case Point(*rest):\n        pass\n";
    let result = parser::parse(src, fid());
    assert!(
        result.is_err(),
        "star inside class pattern should produce a parse error"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("starred") || err_msg.contains("star"),
        "error should mention starred pattern, got: {err_msg}"
    );
}
