use super::*;

#[test]
pub(super) fn test_split_expr_and_spec_nested_brackets() {
    // Colon inside brackets should NOT split
    let (expr, spec) = split_expr_and_spec("d['key']");
    assert_eq!(expr, "d['key']");
    assert!(spec.is_none());
}
