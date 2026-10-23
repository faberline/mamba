use super::*;

#[test]
pub(super) fn test_call_with_star_and_doublestar() {
    match parse_expr_str("f(*a, **b)") {
        Expr::Call { args, .. } => {
            assert_eq!(args.len(), 2);
            assert!(matches!(&args[0], CallArg::StarArg(_)));
            assert!(matches!(&args[1], CallArg::DoubleStarArg(_)));
        }
        other => panic!("expected Call with star args, got {other:?}"),
    }
}
