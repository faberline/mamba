use super::*;

// ── R7: Dict/set literal in expression statement position ─────────────

/// Parsing `{}` as an expression statement should produce a DictLit.
#[test]
pub(super) fn test_empty_dict_literal_as_stmt() {
    match parse_expr_str("{}") {
        Expr::DictLit(entries) => {
            assert!(
                entries.is_empty(),
                "empty dict literal should have 0 entries"
            );
        }
        other => panic!("expected DictLit, got {other:?}"),
    }
}
