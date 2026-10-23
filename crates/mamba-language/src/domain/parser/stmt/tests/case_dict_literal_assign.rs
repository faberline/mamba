use super::*;

#[test]
pub(super) fn test_dict_literal_assign() {
    // Dict literal assignment with entries
    match parse_stmt("d = {'a': 1, 'b': 2}\n") {
        Stmt::Assign { value, .. } => match value.node {
            Expr::DictLit(entries) => assert_eq!(entries.len(), 2),
            other => panic!("expected DictLit, got {other:?}"),
        },
        other => panic!("expected Assign, got {other:?}"),
    }
}
