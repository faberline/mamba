use super::*;

// #1584: chained assignment inside a class body (or any block whose terminator
// is `Dedent`) used to leak desugared continuation stmts (`b = __chained_N__;
// c = __chained_N__; ...`) out of the block. They were popped at module level
// where `__chained_N__` was undefined. Fix: parse_block must drain pending
// desugared stmts into the current block before consuming the Dedent.
#[test]
pub(super) fn test_chained_assign_in_class_body_does_not_leak() {
    let src = "class C:\n    a = 1\n    b = c = d = a\n";
    let module = parse(src);
    // Exactly one top-level stmt — the class def. No leaked `b = ...` etc.
    assert_eq!(
        module.stmts.len(),
        1,
        "chained-assign tail must not leak out of class body"
    );
    match &module.stmts[0].node {
        Stmt::ClassDef { body, .. } => {
            // body holds: a=1, __chained=a, b=__chained, c=__chained, d=__chained
            assert!(
                body.len() >= 4,
                "class body should hold the chained-assign continuations, got {} stmts",
                body.len()
            );
        }
        other => panic!("expected ClassDef, got {other:?}"),
    }
}
