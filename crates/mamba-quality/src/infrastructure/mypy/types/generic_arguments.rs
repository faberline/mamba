//! Parameter specification arguments retain Python's single-parameter shorthand.
use super::super::signatures::{Kind, Parameter};
use super::*;

impl Context {
    pub(in crate::infrastructure::mypy) fn parameter_spec_shorthand(
        &self,
        parameters: &[Type],
        arguments: &[&Expr],
        variables: &VariableMap,
    ) -> bool {
        let [parameter] = parameters else {
            return false;
        };
        if self.is_parameter_spec(*parameter) && arguments.is_empty() {
            return true;
        }
        self.is_parameter_spec(*parameter)
            && !arguments.is_empty()
            && !(arguments.len() == 1
                && (matches!(arguments[0], Expr::List(_) | Expr::EllipsisLiteral(_))
                    || self
                        .annotation_with_variables(arguments[0], variables)
                        .is_some_and(|ty| ty.is_any() || self.is_parameter_spec(ty))))
    }

    pub(super) fn generic_arguments(
        &self,
        parameters: &[Type],
        arguments: &[&Expr],
        variables: &VariableMap,
    ) -> Option<Vec<Type>> {
        if parameters
            .iter()
            .any(|&parameter| self.is_tuple_variable(parameter))
        {
            let required = parameters.iter().filter(|&&parameter| {
                !self.is_tuple_variable(parameter)
                    && self.node(parameter).is_some_and(|node| {
                        matches!(node.as_ref(), Node::Variable(variable) if variable.default.is_none())
                    })
            }).count();
            if arguments.len() < required {
                return Some(
                    parameters
                        .iter()
                        .map(|&parameter| self.default_generic_argument(parameter))
                        .collect(),
                );
            }
            let types = arguments
                .iter()
                .map(|argument| self.annotation_with_variables(argument, variables))
                .collect::<Option<Vec<_>>>()?;
            return Some(self.pack_generic_arguments(parameters, types));
        }
        if arguments.is_empty() && parameters.len() == 1 && self.is_parameter_spec(parameters[0]) {
            return Some(vec![self.intern(Node::Parameters(Vec::new(), false))]);
        }
        if self.parameter_spec_shorthand(parameters, arguments, variables) {
            let captured = arguments
                .iter()
                .map(|argument| {
                    Some(Parameter {
                        name: String::new(),
                        ty: Some(
                            self.annotation_with_variables(argument, variables)
                                .unwrap_or(Type::Any),
                        ),
                        kind: Kind::PositionalOnly,
                        required: true,
                    })
                })
                .collect::<Option<Vec<_>>>()?;
            if captured.len() == 1 && captured[0].ty.is_some_and(|ty| ty.is_any()) {
                return Some(vec![captured[0].ty?]);
            }
            return Some(vec![self.intern(Node::Parameters(captured, false))]);
        }
        arguments
            .iter()
            .enumerate()
            .map(|(index, argument)| {
                if parameters
                    .get(index)
                    .is_some_and(|&ty| self.is_parameter_spec(ty))
                {
                    self.parameter_spec_default(argument, variables)
                } else {
                    self.annotation_with_variables(argument, variables)
                }
            })
            .collect()
    }
}
