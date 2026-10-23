//! Cached properties retain the original settable property declaration.
use super::{Class, Context, MethodKind};
use crate::infrastructure::mypy::types::Variables;
use mamba_language::application::quality_syntax::python_ast::StmtFunctionDef;

pub(super) fn kind(
    class: &mut Class,
    context: &Context,
    function: &StmtFunctionDef,
    variables: &Variables,
) -> MethodKind {
    let cached = function.decorator_list.iter().any(|decorator| {
        context
            .annotation_binding(&decorator.expression, variables)
            .and_then(|ty| context.class_id(ty))
            .is_some_and(|id| context.class(id).namespace == "functools.cached_property")
    });
    if cached {
        class.cached_properties.insert(function.name.to_string());
        MethodKind::Property
    } else {
        super::collect::method_kind(function)
    }
}
