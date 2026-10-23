//! Direct variant type variables retain the original function position errors.
use super::*;
use quality_syntax::python_ast::StmtFunctionDef;

impl Checker<'_> {
    pub(super) fn check_function_variance(
        &mut self,
        function: &StmtFunctionDef,
        implicit_receiver: bool,
        variables: &types::Variables,
    ) {
        let name = function.name.as_str();
        if !matches!(
            name,
            "__init__" | "__new__" | "__post_init__" | "__replace__"
        ) && !(name.starts_with("__") && !name.ends_with("__"))
        {
            for (index, parameter) in function.parameters.iter().enumerate() {
                if index == 0 && implicit_receiver {
                    continue;
                }
                let Some(annotation) = parameter.annotation() else {
                    continue;
                };
                let ty = self
                    .context
                    .annotation_with_variables(annotation, variables);
                if ty.and_then(|ty| self.context.node(ty)).is_some_and(|node| matches!(node.as_ref(), types::Node::Variable(variable) if variable.variance == types::ParameterVariance::Covariant)) {
                    self.emit("misc", "Cannot use a covariant type variable as a parameter".into(), &ruff_text_size::TextRange::at(annotation.start(), ruff_text_size::TextSize::new(1)));
                }
            }
        }
        if let Some(annotation) = function.returns.as_deref() {
            if self.context.annotation_with_variables(annotation, variables).and_then(|ty| self.context.node(ty)).is_some_and(|node| matches!(node.as_ref(), types::Node::Variable(variable) if variable.variance == types::ParameterVariance::Contravariant)) {
                self.emit("misc", "Cannot use a contravariant type variable as return type".into(), &ruff_text_size::TextRange::at(annotation.start(), ruff_text_size::TextSize::new(1)));
            }
        }
    }
}
