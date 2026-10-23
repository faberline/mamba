use super::*;

#[test]
pub(super) fn private_class_bound_keeps_referenced_mangled_binding_alive() {
    let m = desugared("class Foo[__T, __U: __T]:\n    pass\n");
    assert!(restores_or_deletes(&m.stmts, "_Foo__U"));
    assert!(!restores_or_deletes(&m.stmts, "_Foo__T"));
}
