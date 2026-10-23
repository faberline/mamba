use super::*;

#[test]
pub(super) fn test_slice_no_start() {
    match parse_expr_str("a[:3]") {
        Expr::Index { index, .. } => match index.node {
            Expr::Slice { start, stop, step } => {
                assert!(start.is_none());
                assert!(stop.is_some());
                assert!(step.is_none());
            }
            other => panic!("expected Slice, got {other:?}"),
        },
        other => panic!("expected Index(Slice), got {other:?}"),
    }
}
