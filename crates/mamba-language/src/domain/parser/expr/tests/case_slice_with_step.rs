use super::*;

#[test]
pub(super) fn test_slice_with_step() {
    match parse_expr_str("a[::2]") {
        Expr::Index { index, .. } => match index.node {
            Expr::Slice { start, stop, step } => {
                assert!(start.is_none());
                assert!(stop.is_none());
                assert!(step.is_some());
            }
            other => panic!("expected Slice, got {other:?}"),
        },
        other => panic!("expected Index(Slice), got {other:?}"),
    }
}
