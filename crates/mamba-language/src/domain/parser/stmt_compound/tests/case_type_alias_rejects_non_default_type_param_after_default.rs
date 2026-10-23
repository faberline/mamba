use super::*;

#[test]
pub(super) fn test_type_alias_rejects_non_default_type_param_after_default() {
    let err = parser::parse("type Container[T = int, S] = tuple[T, S]\n", fid())
        .expect_err("expected syntax error");
    assert!(
        err.to_string()
            .contains("non-default type parameter follows default type parameter"),
        "unexpected error: {err}"
    );
}
