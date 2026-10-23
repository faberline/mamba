use super::*;

#[test]
pub(super) fn test_split_expr_and_spec_with_spec() {
    let (expr, spec) = split_expr_and_spec("x:.2f");
    assert_eq!(expr, "x");
    assert_eq!(spec.unwrap(), ".2f");
}
