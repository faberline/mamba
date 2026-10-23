use super::*;

// --- Mapping pattern ---

#[test]
pub(super) fn test_mapping_pattern() {
    match parse_pattern("{\"key\": value}") {
        Pattern::Mapping { pairs, rest } => {
            assert_eq!(pairs.len(), 1);
            assert!(rest.is_none());
            match &pairs[0].0.node {
                Expr::StrLit(s) => assert_eq!(s, "key"),
                other => panic!("expected StrLit key, got {other:?}"),
            }
            assert!(matches!(&pairs[0].1.node, Pattern::Binding(n) if n == "value"));
        }
        other => panic!("expected Mapping, got {other:?}"),
    }
}
