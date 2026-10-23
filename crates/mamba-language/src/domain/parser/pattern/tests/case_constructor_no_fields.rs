use super::*;

// --- Constructor ---

#[test]
pub(super) fn test_constructor_no_fields() {
    match parse_pattern("Point()") {
        Pattern::ClassPattern { cls, patterns } => {
            assert_eq!(cls, vec!["Point"]);
            assert!(patterns.is_empty());
        }
        other => panic!("expected ClassPattern, got {other:?}"),
    }
}
