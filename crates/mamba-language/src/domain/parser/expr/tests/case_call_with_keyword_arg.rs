use super::*;

#[test]
pub(super) fn test_call_with_keyword_arg() {
    match parse_expr_str("f(x=1)") {
        Expr::Call { args, .. } => {
            assert_eq!(args.len(), 1);
            match &args[0] {
                CallArg::Keyword { name, value } => {
                    assert_eq!(name, "x");
                    assert!(matches!(value.node, Expr::IntLit(1)));
                }
                other => panic!("expected Keyword arg, got {other:?}"),
            }
        }
        other => panic!("expected Call, got {other:?}"),
    }
}
