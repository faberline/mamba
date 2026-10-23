use super::*;

#[test]
pub(super) fn test_constructor_with_fields() {
    // Since Fix #827, positional args are parsed as ClassPattern with (None, pattern)
    match parse_pattern("Circle(r)") {
        Pattern::ClassPattern { cls, patterns } => {
            assert_eq!(cls, vec!["Circle"]);
            assert_eq!(patterns.len(), 1);
            assert_eq!(patterns[0].0, None);
            assert!(matches!(&patterns[0].1.node, Pattern::Binding(n) if n == "r"));
        }
        other => panic!("expected ClassPattern, got {other:?}"),
    }
}
