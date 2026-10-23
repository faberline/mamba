use super::*;

#[test]
pub(super) fn test_mapping_pattern_rest_only() {
    match parse_pattern("{**rest}") {
        Pattern::Mapping { pairs, rest } => {
            assert!(pairs.is_empty());
            assert_eq!(rest.as_deref(), Some("rest"));
        }
        other => panic!("expected Mapping, got {other:?}"),
    }
}
