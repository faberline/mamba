use super::*;

#[test]
pub(super) fn test_for_list_display_target() {
    // #1927 — `[a, b]` is an equally-valid for-target delimiter as
    // `(a, b)`; as the sole target unit it flattens directly into
    // `targets`, same as the bare-comma and paren-group forms.
    match parse_stmt("for [k, v] in items:\n    pass\n") {
        Stmt::For { targets, .. } => {
            assert_eq!(targets, vec!["k", "v"]);
        }
        other => panic!("expected For, got {other:?}"),
    }
}
