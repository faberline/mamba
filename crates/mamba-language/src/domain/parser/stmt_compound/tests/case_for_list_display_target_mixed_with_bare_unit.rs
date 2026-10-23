use super::*;

#[test]
pub(super) fn test_for_list_display_target_mixed_with_bare_unit() {
    // #1927 — a `[...]` group alongside a bare unit exercises the
    // multi-unit placeholder/prepend desugaring path (same as an
    // equivalent `(a, b), c` paren-group case), not just the
    // single-unit direct-flatten path above.
    match parse_stmt("for [a, b], c in items:\n    pass\n") {
        Stmt::For { targets, body, .. } => {
            assert_eq!(targets.len(), 2);
            assert_eq!(targets[1], "c");
            assert_eq!(body.len(), 2, "expected a prepended unpack assign + pass");
            match &body[0].node {
                Stmt::Assign { target, .. } => match &target.node {
                    Expr::TupleLit(elems) => {
                        assert!(matches!(&elems[0].node, Expr::Ident(n) if n == "a"));
                        assert!(matches!(&elems[1].node, Expr::Ident(n) if n == "b"));
                    }
                    other => panic!("expected TupleLit target, got {other:?}"),
                },
                other => panic!("expected prepended Assign, got {other:?}"),
            }
        }
        other => panic!("expected For, got {other:?}"),
    }
}
