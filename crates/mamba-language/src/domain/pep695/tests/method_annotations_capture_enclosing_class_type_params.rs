use super::*;

#[test]
pub(super) fn method_annotations_capture_enclosing_class_type_params() {
    let m = desugared("class Foo[T]:\n    def meth(self, arg: T):\n        return arg\n");
    assert!(m.stmts.iter().any(|stmt| {
        matches!(
            &stmt.node,
            Stmt::Assign { target, value }
                if matches!(
                    &target.node,
                    Expr::Attr { object, attr }
                        if attr == "__mb_class_type_params__"
                            && matches!(
                                &object.node,
                                Expr::Attr { object: cls, attr: meth }
                                    if meth == "meth"
                                        && matches!(&cls.node, Expr::Ident(name) if name == "Foo")
                            )
                )
                    && matches!(
                        &value.node,
                        Expr::Attr { object, attr }
                            if attr == "__type_params__"
                                && matches!(&object.node, Expr::Ident(name) if name == "Foo")
                    )
        )
    }));
}
