//! Callable inputs bind legacy ParamSpec variables; component annotations do not.
use super::*;
use mamba_language::application::quality_syntax::python_ast::TypeParam;

impl Signature {
    pub(super) fn bound_parameters(
        function: &StmtFunctionDef,
        context: &types::Context,
        parameters: &[Parameter],
        generic: Vec<Type>,
    ) -> Vec<Type> {
        generic.into_iter().filter(|&ty| {
            if !context.is_parameter_spec(ty) { return true; }
            let explicit = function.type_params.as_ref().is_some_and(|parameters| parameters.type_params.iter().any(|parameter| {
                matches!(parameter, TypeParam::ParamSpec(parameter) if context.name(ty) == parameter.name.as_str())
            }));
            explicit || parameters.iter().filter_map(|parameter| parameter.ty).any(|input| context.parameter_spec_callable_input(input, ty))
        }).collect()
    }
}
