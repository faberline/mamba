use super::*;

#[test]
pub(super) fn test_try_set_method_call_in_body() {
    // S-TRY-DICT-2: set literal method call in try body
    match parse_stmt("try:\n    s = {1, 2}.remove(99)\nexcept KeyError:\n    pass\n") {
        Stmt::Try { body, handlers, .. } => {
            assert_eq!(body.len(), 1);
            match &body[0].node {
                Stmt::Assign { target, value } => {
                    assert!(matches!(&target.node, Expr::Ident(n) if n == "s"));
                    // value should be Call(Attr(SetLit([1,2]), "remove"), [99])
                    match &value.node {
                        Expr::Call { func, args } => {
                            match &func.node {
                                Expr::Attr { object, attr } => {
                                    match &object.node {
                                        Expr::SetLit(elems) => assert_eq!(elems.len(), 2),
                                        other => panic!("expected SetLit, got {other:?}"),
                                    }
                                    assert_eq!(attr, "remove");
                                }
                                other => panic!("expected Attr, got {other:?}"),
                            }
                            assert_eq!(args.len(), 1);
                        }
                        other => panic!("expected Call, got {other:?}"),
                    }
                }
                other => panic!("expected Assign, got {other:?}"),
            }
            assert_eq!(handlers.len(), 1);
        }
        other => panic!("expected Try, got {other:?}"),
    }
}
