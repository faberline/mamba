use super::*;

#[test]
pub(super) fn test_type_alias_union() {
    match parse_stmt("type Number = int | float\n") {
        Stmt::TypeAlias { value, .. } => {
            assert!(matches!(
                value.node,
                Expr::BinOp {
                    op: BinOp::BitOr,
                    ..
                }
            ));
        }
        other => panic!("expected TypeAlias(Union), got {other:?}"),
    }
}
