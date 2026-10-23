//! Alias use diagnostics remain separate from their transparent target type.
use super::*;
use quality_syntax::python_ast::ExprCall;

impl Checker<'_> {
    pub(super) fn check_alias_annotation(&mut self, expression: &Expr) -> bool {
        let (base, arguments) = if let Expr::Subscript(value) = expression {
            let arguments: Vec<_> = match value.slice.as_ref() {
                Expr::Tuple(tuple) => tuple.elts.iter().collect(),
                value => vec![value],
            };
            (value.value.as_ref(), Some(arguments))
        } else {
            (expression, None)
        };
        let variables = self.scopes.variables();
        let Some(id) = self
            .context
            .annotation_binding(base, &variables)
            .and_then(|ty| self.context.alias_id(ty))
        else {
            return false;
        };
        let alias = self.context.alias(id);
        if alias.invalid_variable.is_some() {
            if self.context.recursive_alias(id)
                && !self
                    .invalid_alias_uses
                    .insert((id, self.functions.len(), self.classes.len()))
            {
                return true;
            }
            self.emit_with_hint("valid-type", format!("Variable \"{}\" is not valid as a type", self.scopes.qualified_name(&alias.name)), &ruff_text_size::TextRange::at(expression.start(), ruff_text_size::TextSize::new(1)), Some("See https://mypy.readthedocs.io/en/stable/common_issues.html#variables-vs-type-aliases".into()));
            return true;
        }
        let required = alias.parameters.iter().filter(|&&ty| self.context.node(ty).is_some_and(|node| matches!(node.as_ref(), types::Node::Variable(variable) if variable.default.is_none()))).count();
        let Some(arguments) = arguments else {
            if required != 0 {
                let range = if alias.bare_class.is_some() {
                    expression.range()
                } else {
                    ruff_text_size::TextRange::at(
                        expression.start(),
                        ruff_text_size::TextSize::new(1),
                    )
                };
                self.emit(
                    "type-arg",
                    format!("Missing type arguments for generic type \"{}\"", alias.name),
                    &range,
                );
            }
            return true;
        };
        if !self
            .context
            .parameter_spec_shorthand(&alias.parameters, &arguments, &variables)
            && !self.context.generic_argument_count_valid(
                &alias.parameters,
                arguments.len(),
                required,
            )
        {
            self.emit(
                "type-arg",
                format!(
                    "Bad number of arguments for type alias, expected {}, given {}",
                    if required == alias.parameters.len() {
                        required.to_string()
                    } else {
                        format!("between {required} and {}", alias.parameters.len())
                    },
                    arguments.len()
                ),
                expression,
            );
            return true;
        }
        for (&parameter, argument) in alias.parameters.iter().zip(arguments) {
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
            let Some(actual) = self.scopes.annotation(argument) else {
                continue;
            };
            let point =
                ruff_text_size::TextRange::at(argument.start(), ruff_text_size::TextSize::new(1));
            if !actual.is_any() && !variable.values.is_empty() && !variable.values.contains(&actual)
            {
                self.emit(
                    "type-var",
                    format!(
                        "Value of type variable \"{}\" of \"{}\" cannot be \"{}\"",
                        variable.name,
                        alias.name,
                        self.context.name(actual)
                    ),
                    &point,
                );
            } else if !self.context.accepts(variable.bound, actual) {
                self.emit(
                    "type-var",
                    format!(
                        "Type argument \"{}\" of \"{}\" must be a subtype of \"{}\"",
                        self.context.name(actual),
                        alias.name,
                        self.context.name(variable.bound)
                    ),
                    &point,
                );
            }
        }
        true
    }

    pub(super) fn check_type_alias_declaration(&mut self, statement: &Stmt) {
        if let Stmt::TypeAlias(alias) = statement {
            self.check_type_default_version(alias.type_params.as_deref(), alias);
            self.check_parameter_spec_defaults(alias.type_params.as_deref());
            self.check_tuple_variable_defaults(alias.type_params.as_deref());
        }
        let name = match statement {
            Stmt::Assign(value) => value.targets.first(),
            Stmt::AnnAssign(value) => Some(value.target.as_ref()),
            Stmt::TypeAlias(value) => Some(value.name.as_ref()),
            _ => None,
        };
        let value = match statement {
            Stmt::Assign(value) => Some(value.value.as_ref()),
            Stmt::AnnAssign(value) => value.value.as_deref(),
            Stmt::TypeAlias(value) => Some(value.value.as_ref()),
            _ => None,
        };
        if let Some(id) = name
            .and_then(|name| {
                self.context
                    .annotation_binding(name, &self.scopes.variables())
            })
            .and_then(|ty| self.context.alias_id(ty))
        {
            let alias = self.context.alias(id);
            self.check_recursive_alias_declaration(id, statement, value);
            let mut previous = None;
            for &ty in &alias.parameters {
                let node = self.context.node(ty).unwrap();
                let types::Node::Variable(variable) = node.as_ref() else {
                    continue;
                };
                if variable.default.is_some() {
                    previous = Some(variable.name.clone());
                    if alias.modern {
                        self.check_variable_default(ty, statement);
                    }
                } else if let Some(previous) = &previous {
                    if let Some(value) = value {
                        self.emit("misc", format!("\"{}\" cannot appear after \"{previous}\" in type parameter list because it has no default type", variable.name), &ruff_text_size::TextRange::at(value.start(), ruff_text_size::TextSize::new(1)));
                    }
                }
            }
        }
        if let Some((variable, range)) = name
            .and_then(|name| {
                self.context
                    .annotation_binding(name, &self.scopes.variables())
            })
            .and_then(|ty| self.context.alias_id(ty))
            .and_then(|id| self.context.alias(id).invalid_variable.clone())
        {
            self.emit(
                "misc",
                format!(
                    "Type variable \"{}\" is invalid as target for type alias",
                    self.scopes.qualified_name(&variable)
                ),
                &ruff_text_size::TextRange::at(range.start(), ruff_text_size::TextSize::new(1)),
            );
        }
    }

    pub(super) fn check_type_alias_call(&mut self, call: &ExprCall) -> bool {
        let Some(marker) = self
            .scopes
            .constructor_alias_marker(&call.func)
            .or_else(|| self.infer(&call.func))
        else {
            return false;
        };
        let Some(id) = self.context.alias_id(marker) else {
            return false;
        };
        let alias = self.context.alias(id);
        if alias.modern {
            self.emit("operator", "\"TypeAliasType\" not callable".into(), call);
            return true;
        }
        let fixed_tuple = self.context.builtin_fixed_tuple_alias(marker);
        if alias.explicit_arguments
            && (fixed_tuple
                || self
                    .context
                    .builtin_alias_constructor(marker, self.python_target)
                    .is_some())
        {
            self.check_alias_annotation(&call.func);
        }
        if fixed_tuple {
            self.emit(
                "operator",
                "\"<typing special form>\" not callable".into(),
                call,
            );
            return true;
        }
        false
    }
}
