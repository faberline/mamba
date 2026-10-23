//! Access checks retain the nearest common statement and caught-exception boundary.
use mamba_language::application::quality_syntax::python_ast::Expr;

#[derive(Clone, Copy)]
pub(super) enum Part {
    Other,
    Body,
    Handler(usize, bool),
    Else,
    Finally,
}
pub(super) type Parents = Vec<(u32, Part)>;

pub(super) fn catches(value: Option<&Expr>) -> bool {
    match value {
        None => true,
        Some(Expr::Name(value)) => {
            matches!(
                value.id.as_str(),
                "AttributeError" | "Exception" | "BaseException"
            )
        }
        Some(Expr::Tuple(value)) => value.elts.iter().any(|value| catches(Some(value))),
        _ => false,
    }
}
pub(super) fn exclusive(access: &Parents, definition: &Parents) -> bool {
    for (id, access) in access.iter().rev() {
        let Some((_, definition)) = definition.iter().find(|(other, _)| other == id) else {
            continue;
        };
        return match (access, definition) {
            (Part::Body, Part::Handler(_, true)) | (Part::Handler(_, true), Part::Body) => true,
            (Part::Else, Part::Handler(_, _)) | (Part::Handler(_, _), Part::Else) => true,
            (Part::Handler(left, _), Part::Handler(right, _)) => left != right,
            _ => false,
        };
    }
    false
}
