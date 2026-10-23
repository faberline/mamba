use super::*;

// --- Mapping pattern with rest capture (#827) ---

#[test]
pub(super) fn test_mapping_pattern_with_rest() {
    match parse_pattern("{\"type\": kind, **rest}") {
        Pattern::Mapping { pairs, rest } => {
            assert_eq!(pairs.len(), 1);
            assert_eq!(rest.as_deref(), Some("rest"));
            match &pairs[0].0.node {
                Expr::StrLit(s) => assert_eq!(s, "type"),
                other => panic!("expected StrLit key, got {other:?}"),
            }
            assert!(matches!(&pairs[0].1.node, Pattern::Binding(n) if n == "kind"));
        }
        other => panic!("expected Mapping, got {other:?}"),
    }
}
