use super::*;

// --- Binding ---

#[test]
pub(super) fn test_binding_pattern() {
    match parse_pattern("x") {
        Pattern::Binding(name) => assert_eq!(name, "x"),
        other => panic!("expected Binding, got {other:?}"),
    }
}
