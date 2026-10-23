use super::*;

// #1594: nested-paren for-target — `for (a, b), c in ...` and
// `for (a, b), (c, d) in ...` desugar to fresh `__for_target_N_K__`
// flat names plus prepended unpack assigns at the start of the body.
#[test]
pub(super) fn test_for_nested_paren_target() {
    let module = parse(
        "for (a, b), c in items:\n\
         \x20   pass\n",
    );
    match &module.stmts[0].node {
        Stmt::For { targets, body, .. } => {
            assert_eq!(targets.len(), 2);
            assert!(
                targets[0].starts_with("__for_target_"),
                "first target must be synthetic, got {:?}",
                targets[0]
            );
            assert_eq!(targets[1], "c");
            assert!(
                matches!(&body[0].node, Stmt::Assign { .. }),
                "expected prepended unpack assign as first body stmt"
            );
        }
        _ => panic!("expected For"),
    }
}
