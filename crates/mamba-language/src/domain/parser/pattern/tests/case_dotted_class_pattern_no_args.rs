use super::*;

#[test]
pub(super) fn test_dotted_class_pattern_no_args() {
    match parse_pattern("Shape.Circle()") {
        Pattern::ClassPattern { cls, patterns } => {
            assert_eq!(cls, vec!["Shape", "Circle"]);
            assert!(patterns.is_empty());
        }
        other => panic!("expected ClassPattern, got {other:?}"),
    }
}
