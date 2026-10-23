use super::*;

#[test]
pub(super) fn generic_class_injects_parameters_too() {
    let m = desugared("class C[T]:\n    pass\n");
    let (target, _) = find_typevar_assign(&m.stmts, "T").expect("T typevar assignment");
    assert!(matches!(&target.node, Expr::Ident(name) if name == "T"));
    let attrs: Vec<&str> = m
        .stmts
        .iter()
        .filter_map(|s| match &s.node {
            Stmt::Assign { target, .. } => match &target.node {
                Expr::Attr { attr, .. } => Some(attr.as_str()),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(attrs, vec!["__type_params__", "__parameters__"]);
    assert!(restores_or_deletes(&m.stmts, "T"));
}
