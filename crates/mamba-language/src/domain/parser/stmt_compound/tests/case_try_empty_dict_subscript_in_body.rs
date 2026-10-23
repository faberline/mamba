use super::*;

// --- Try with dict/set literals in body (#1112) ---

#[test]
pub(super) fn test_try_empty_dict_subscript_in_body() {
    // S-TRY-DICT-1: empty dict subscript in try body
    match parse_stmt("try:\n    d = {}['x']\nexcept KeyError:\n    pass\n") {
        Stmt::Try { body, handlers, .. } => {
            assert_eq!(body.len(), 1);
            match &body[0].node {
                Stmt::Assign { target, value } => {
                    assert!(matches!(&target.node, Expr::Ident(n) if n == "d"));
                    match &value.node {
                        Expr::Index { object, index } => {
                            assert!(
                                matches!(&object.node, Expr::DictLit(entries) if entries.is_empty())
                            );
                            assert!(matches!(&index.node, Expr::StrLit(s) if s == "x"));
                        }
                        other => panic!("expected Index, got {other:?}"),
                    }
                }
                other => panic!("expected Assign, got {other:?}"),
            }
            assert_eq!(handlers.len(), 1);
            assert!(matches!(
                &handlers[0].exc_type.as_ref().unwrap().node,
                Expr::Ident(n) if n == "KeyError"
            ));
        }
        other => panic!("expected Try, got {other:?}"),
    }
}
