use super::*;

// --- Type alias ---

#[test]
pub(super) fn test_type_alias_simple() {
    match parse_stmt("type Number = int\n") {
        Stmt::TypeAlias {
            name,
            type_params,
            value,
        } => {
            assert_eq!(name, "Number");
            assert!(type_params.is_empty());
            assert!(matches!(value.node, Expr::Ident(ref n) if n == "int"));
        }
        other => panic!("expected TypeAlias, got {other:?}"),
    }
}
