use super::*;

#[test]
pub(super) fn test_class_pattern_positional_after_keyword_rejected() {
    // PEP 634: positional argument may not follow keyword argument
    let src = "match x:\n    case Point(x=1, 2):\n        pass\n";
    let result = parser::parse(src, fid());
    assert!(
        result.is_err(),
        "positional after keyword should produce a parse error"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("positional") || err_msg.contains("keyword"),
        "error should mention positional/keyword ordering, got: {err_msg}"
    );
}
