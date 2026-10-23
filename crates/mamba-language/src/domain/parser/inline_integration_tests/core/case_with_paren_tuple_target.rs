use super::*;

// #1592: parenthesized tuple-target in `with X as (a, b):` — desugars to
// a fresh `__with_target_N__` alias plus a tuple-unpack assign prepended
// to the body.
#[test]
pub(super) fn test_with_paren_tuple_target() {
    let module = parse(
        "def make(): return (1, 2)\n\
         with make() as (a, b):\n\
         \x20   pass\n",
    );
    match &module.stmts[1].node {
        Stmt::With { items, body } => {
            assert_eq!(items.len(), 1);
            let alias = items[0].alias.as_deref().expect("alias must be set");
            assert!(
                alias.starts_with("__with_target_"),
                "expected synthetic alias, got {alias:?}"
            );
            // First body stmt must be the unpack assign.
            assert!(!body.is_empty());
            assert!(
                matches!(&body[0].node, Stmt::Assign { .. }),
                "expected prepended unpack assign as first body stmt"
            );
        }
        other => panic!("expected With, got {other:?}"),
    }
}
