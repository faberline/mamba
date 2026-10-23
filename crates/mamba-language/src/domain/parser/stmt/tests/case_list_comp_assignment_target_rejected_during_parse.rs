use super::*;

#[test]
pub(super) fn test_list_comp_assignment_target_rejected_during_parse() {
    let err = parser::parse("[y for y in (1, 2)] = 10\n", fid())
        .expect_err("list comprehension cannot be an assignment target");
    assert!(err.to_string().contains("cannot assign"), "{err}");
}
