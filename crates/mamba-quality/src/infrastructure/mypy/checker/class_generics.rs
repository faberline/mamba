//! Generic class annotations retain strict arity and bound diagnostics.
use super::*;

impl Checker<'_> {
    pub(super) fn check_class_annotation(&mut self, expression: &Expr) -> bool {
        self.check_class_annotation_at(expression, expression.range())
    }

    pub(super) fn check_class_annotation_at(
        &mut self,
        expression: &Expr,
        range: ruff_text_size::TextRange,
    ) -> bool {
        let (base, arguments) = if let Expr::Subscript(subscript) = expression {
            let arguments: Vec<_> = match subscript.slice.as_ref() {
                Expr::Tuple(tuple) => tuple.elts.iter().collect(),
                argument => vec![argument],
            };
            (subscript.value.as_ref(), Some(arguments))
        } else if matches!(expression, Expr::Name(_) | Expr::Attribute(_)) {
            (expression, None)
        } else {
            return false;
        };
        let Some(id) = self
            .scopes
            .annotation(base)
            .filter(|&ty| !self.context.is_source_self_variable(ty))
            .and_then(|ty| self.context.class_id(ty))
        else {
            return false;
        };
        let class = self.context.class(id);
        if class.generic.is_empty() {
            if let Some(arguments) = arguments {
                self.emit(
                    "type-arg",
                    format!(
                        "\"{}\" expects no type arguments, but {} given",
                        class.name,
                        arguments.len()
                    ),
                    &range,
                );
                return true;
            }
            return false;
        }
        let Some(arguments) = arguments else {
            if class.generic.iter().any(|&ty| self.context.node(ty).is_some_and(|node| matches!(node.as_ref(), types::Node::Variable(variable) if variable.default.is_none()))) {
                self.emit("type-arg", format!("Missing type arguments for generic type \"{}\"", class.name), &range);
            }
            return true;
        };
        let minimum = class.generic.iter().filter(|&&ty| self.context.node(ty).is_some_and(|node| matches!(node.as_ref(), types::Node::Variable(variable) if variable.default.is_none()))).count();
        if !self.context.parameter_spec_shorthand(
            &class.generic,
            &arguments,
            &self.scopes.variables(),
        ) && !self
            .context
            .generic_argument_count_valid(&class.generic, arguments.len(), minimum)
        {
            if class
                .generic
                .iter()
                .any(|&ty| self.context.is_tuple_variable(ty))
            {
                self.emit(
                    "type-arg",
                    format!(
                        "Bad number of arguments, expected: at least {}, given: {}",
                        minimum.saturating_sub(1),
                        arguments.len()
                    ),
                    &range,
                );
                self.emit(
                    "type-arg",
                    format!("Missing type arguments for generic type \"{}\"", class.name),
                    &range,
                );
                return true;
            }
            self.emit(
                "type-arg",
                format!(
                    "\"{}\" expects {} type argument{}, but {} given",
                    class.name,
                    if minimum == class.generic.len() {
                        minimum.to_string()
                    } else {
                        format!("between {minimum} and {}", class.generic.len())
                    },
                    if class.generic.len() == 1 { "" } else { "s" },
                    if arguments.is_empty() {
                        "none".into()
                    } else {
                        arguments.len().to_string()
                    }
                ),
                &range,
            );
            if arguments.is_empty() {
                self.emit(
                    "type-arg",
                    format!("Missing type arguments for generic type \"{}\"", class.name),
                    &range,
                );
            }
            return true;
        }
        for (&parameter, argument) in class.generic.iter().zip(arguments) {
            if self.context.is_tuple_variable(parameter) {
                continue;
            }
            if self.context.is_parameter_spec(parameter) {
                if let Expr::List(items) = argument {
                    for item in &items.elts {
                        self.check_callable_annotation(item);
                    }
                } else if !argument.is_ellipsis_literal_expr()
                    && !self
                        .scopes
                        .annotation(argument)
                        .is_some_and(|ty| self.context.is_parameter_spec(ty))
                {
                    self.check_callable_annotation(argument);
                }
                continue;
            }
            self.check_callable_annotation(argument);
            let Some(node) = self.context.node(parameter) else {
                continue;
            };
            let types::Node::Variable(variable) = node.as_ref() else {
                continue;
            };
            if let Some(actual) = self.scopes.annotation(argument) {
                if !actual.is_any()
                    && !variable.values.is_empty()
                    && !variable.values.contains(&actual)
                {
                    if self
                        .context
                        .node(actual)
                        .is_some_and(|node| matches!(node.as_ref(), types::Node::Union(_)))
                    {
                        self.emit_column_zero(
                            "type-var",
                            format!("Invalid type argument value for \"{}\"", class.name),
                            expression,
                        );
                    } else {
                        self.emit(
                            "type-var",
                            format!(
                                "Value of type variable \"{}\" of \"{}\" cannot be \"{}\"",
                                variable.name,
                                class.name,
                                self.context.name(actual)
                            ),
                            &ruff_text_size::TextRange::at(
                                argument.start(),
                                ruff_text_size::TextSize::new(1),
                            ),
                        );
                    }
                } else if !self.context.accepts(variable.bound, actual) {
                    self.emit(
                        "type-var",
                        format!(
                            "Type argument \"{}\" of \"{}\" must be a subtype of \"{}\"",
                            self.context.name(actual),
                            class.name,
                            self.context.name(variable.bound)
                        ),
                        &ruff_text_size::TextRange::at(
                            argument.start(),
                            ruff_text_size::TextSize::new(1),
                        ),
                    );
                }
            }
        }
        true
    }
}
