use super::*;

#[test]
pub(super) fn test_try_dict_literal_subscript_in_body() {
    // S-TRY-DICT-3: dict literal subscript in try body
    match parse_stmt("try:\n    x = {'a': 1}['a']\nexcept:\n    pass\n") {
        Stmt::Try { body, handlers, .. } => {
            assert_eq!(body.len(), 1);
            match &body[0].node {
                Stmt::Assign { target, value } => {
                    assert!(matches!(&target.node, Expr::Ident(n) if n == "x"));
                    match &value.node {
                        Expr::Index { object, index } => {
                            match &object.node {
                                Expr::DictLit(entries) => assert_eq!(entries.len(), 1),
                                other => panic!("expected DictLit, got {other:?}"),
                            }
                            assert!(matches!(&index.node, Expr::StrLit(s) if s == "a"));
                        }
                        other => panic!("expected Index, got {other:?}"),
                    }
                }
                other => panic!("expected Assign, got {other:?}"),
            }
            assert_eq!(handlers.len(), 1);
            // Bare except (no type)
            assert!(handlers[0].exc_type.is_none());
        }
        other => panic!("expected Try, got {other:?}"),
    }
}
