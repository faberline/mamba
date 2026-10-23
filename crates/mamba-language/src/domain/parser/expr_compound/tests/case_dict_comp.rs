use super::*;

#[test]
pub(super) fn test_dict_comp() {
    match parse_expr("{k: v for k, v in items}") {
        Expr::DictComp { generators, .. } => {
            assert_eq!(generators.len(), 1);
            assert_eq!(generators[0].targets, vec!["k", "v"]);
        }
        other => panic!("expected DictComp, got {other:?}"),
    }
}
