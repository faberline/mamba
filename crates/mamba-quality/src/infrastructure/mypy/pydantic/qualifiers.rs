//! Field qualifiers follow the resolved typing symbol, including import aliases.
use super::types::{Context, Variables};
use mamba_language::application::quality_syntax::python_ast::Expr;

pub(in crate::infrastructure::mypy) fn form(
    annotation: &Expr,
    context: &Context,
    variables: &Variables,
) -> Option<String> {
    let expression = if let Expr::Subscript(subscript) = annotation {
        subscript.value.as_ref()
    } else {
        annotation
    };
    context.typing_form(expression, variables)
}
