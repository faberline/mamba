use super::*;

// --- Class pattern ---

#[test]
pub(super) fn test_class_pattern() {
    match parse_pattern("Point(x=1, y=2)") {
        Pattern::ClassPattern { cls, patterns } => {
            assert_eq!(cls, vec!["Point"]);
            assert_eq!(patterns.len(), 2);
            assert_eq!(patterns[0].0.as_deref(), Some("x"));
            assert_eq!(patterns[1].0.as_deref(), Some("y"));
        }
        other => panic!("expected ClassPattern, got {other:?}"),
    }
}
