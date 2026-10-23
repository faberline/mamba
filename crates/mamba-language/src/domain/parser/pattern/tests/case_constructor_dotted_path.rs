use super::*;

#[test]
pub(super) fn test_constructor_dotted_path() {
    match parse_pattern("Shape.Circle") {
        Pattern::Constructor { path, fields } => {
            assert_eq!(path, vec!["Shape", "Circle"]);
            assert!(fields.is_empty());
        }
        other => panic!("expected Constructor, got {other:?}"),
    }
}
