use super::*;

// ── R7: Dict/set literal parsing in compound expressions ──────────────

/// Parse `{1: 'a', 2: 'b'}` as a dict literal with integer keys.
#[test]
pub(super) fn test_dict_literal_int_keys() {
    match parse_expr("{1: 'a', 2: 'b'}") {
        Expr::DictLit(entries) => {
            assert_eq!(entries.len(), 2);
            // Each entry has (Some(key), value)
            assert!(entries[0].0.is_some());
            assert!(entries[1].0.is_some());
        }
        other => panic!("expected DictLit, got {other:?}"),
    }
}
