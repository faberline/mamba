use super::*;

/// Parse dict comprehension `{k: v for k, v in items}`.
#[test]
pub(super) fn test_dict_comp_in_compound() {
    match parse_expr("{k: v for k, v in items}") {
        Expr::DictComp { .. } => {}
        other => panic!("expected DictComp, got {other:?}"),
    }
}
