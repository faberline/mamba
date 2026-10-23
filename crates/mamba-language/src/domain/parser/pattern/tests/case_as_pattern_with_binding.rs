use super::*;

// --- AS pattern (#827) ---

#[test]
pub(super) fn test_as_pattern_with_binding() {
    match parse_pattern("x as y") {
        Pattern::As { pattern, name } => {
            assert_eq!(name, "y");
            assert!(matches!(&pattern.node, Pattern::Binding(n) if n == "x"));
        }
        other => panic!("expected As, got {other:?}"),
    }
}
