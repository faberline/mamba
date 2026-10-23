use super::*;

/// Parse `{'key': True}` as a single-entry dict literal.
#[test]
pub(super) fn test_dict_literal_single_entry() {
    match parse_expr("{'key': True}") {
        Expr::DictLit(entries) => {
            assert_eq!(entries.len(), 1);
            let (ref key, ref val) = entries[0];
            assert!(key.is_some());
            assert!(matches!(val.node, Expr::BoolLit(true)));
        }
        other => panic!("expected DictLit, got {other:?}"),
    }
}
