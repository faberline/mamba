use super::*;

#[test]
pub(super) fn test_list_comp_records_target_read_before_bind() {
    match parse_expr("[1 for l[0], l in items]") {
        Expr::ListComp { generators, .. } => {
            assert_eq!(generators[0].targets, vec!["l"]);
            assert_eq!(generators[0].target_reads_before_bind, vec!["l"]);
            assert!(generators[0].unpack_target);
        }
        other => panic!("expected ListComp, got {other:?}"),
    }
}
