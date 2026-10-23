use super::*;

#[test]
pub(super) fn test_list_comp_rejects_walrus_rebinding_complex_target_name() {
    let src = "[(a := 1) for a, (*b, c[d+e::f(g)], h.i) in j]\n";
    let err = crate::parser::parse(src, fid()).expect_err("expected syntax error");
    assert!(
        err.to_string()
            .contains("assignment expression cannot rebind comprehension iteration variable 'a'"),
        "{err}"
    );
}
