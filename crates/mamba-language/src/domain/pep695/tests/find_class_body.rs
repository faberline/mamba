use super::*;

pub(super) fn find_class_body<'a>(stmts: &'a [Spanned<Stmt>], name: &str) -> &'a [Spanned<Stmt>] {
    stmts
        .iter()
        .find_map(|stmt| match &stmt.node {
            Stmt::ClassDef {
                name: class_name,
                body,
                ..
            } if class_name == name => Some(body.as_slice()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected class {name}"))
}
