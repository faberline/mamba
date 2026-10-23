use super::*;

#[test]
pub(super) fn test_split_expr_and_spec_no_spec() {
    let (expr, spec) = split_expr_and_spec("name");
    assert_eq!(expr, "name");
    assert!(spec.is_none());
}
