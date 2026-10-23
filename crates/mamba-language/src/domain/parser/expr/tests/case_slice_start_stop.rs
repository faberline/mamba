use super::*;

#[test]
pub(super) fn test_slice_start_stop() {
    match parse_expr_str("a[1:3]") {
        Expr::Index { index, .. } => match index.node {
            Expr::Slice { start, stop, step } => {
                assert!(start.is_some());
                assert!(stop.is_some());
                assert!(step.is_none());
            }
            other => panic!("expected Slice, got {other:?}"),
        },
        other => panic!("expected Index(Slice), got {other:?}"),
    }
}
