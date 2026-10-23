//! Parameter specification errors distinguish scope, position, and paired components.
use super::*;
use quality_syntax::python_ast::{StmtAssign, StmtFunctionDef, TypeParam, TypeParams};

impl Checker<'_> {
    fn parameter_spec_bound(&self, ty: types::Type) -> bool {
        self.scopes.type_variable_bound(ty)
            || self.class_types.iter().flatten().any(|&class| {
                self.context
                    .class_id(class)
                    .is_some_and(|id| self.context.class(id).generic.contains(&ty))
            })
    }

    pub(super) fn check_parameter_spec_use(&mut self, expression: &Expr) -> bool {
        let Some(ty) = self.scopes.annotation(expression) else {
            return false;
        };
        let Some(node) = self.context.node(ty) else {
            return false;
        };
        let (variable, component) = match node.as_ref() {
            types::Node::Variable(variable) if variable.parameter_spec => (ty, false),
            types::Node::ParamSpecArgs(variable) | types::Node::ParamSpecKwargs(variable) => {
                (*variable, true)
            }
            _ => return false,
        };
        let bound = self.parameter_spec_bound(variable);
        if component && bound && self.annotation_allow_component {
            return true;
        }
        if self.annotation_in_signature && !self.parameter_spec_errors.insert(variable) {
            return true;
        }
        let message = if component && bound {
            "ParamSpec components are not allowed here".into()
        } else if !component && (bound || self.annotation_in_signature) {
            format!(
                "Invalid location for ParamSpec \"{}\"",
                self.context.name(variable)
            )
        } else {
            format!("ParamSpec \"{}\" is unbound", self.context.name(variable))
        };
        let hint = (!component && (bound || self.annotation_in_signature)).then(|| format!("You can use ParamSpec as the first argument to Callable, e.g., \"Callable[{}, int]\"", self.context.name(variable)));
        self.emit_with_hint("valid-type", message, expression, hint);
        true
    }

    pub(super) fn check_parameter_spec_callable(&mut self, expression: &Expr) -> bool {
        let Expr::Subscript(subscript) = expression else {
            return false;
        };
        let name = match subscript.value.as_ref() {
            Expr::Name(name) => name.id.as_str(),
            Expr::Attribute(attribute) => attribute.attr.as_str(),
            _ => return false,
        };
        let variables = self.scopes.variables();
        if matches!(
            self.context
                .typing_form(&subscript.value, &variables)
                .as_deref(),
            Some("Generic" | "Protocol")
        ) {
            let arguments: Vec<_> = match subscript.slice.as_ref() {
                Expr::Tuple(tuple) => tuple.elts.iter().collect(),
                expression => vec![expression],
            };
            for argument in arguments {
                if !self
                    .scopes
                    .annotation(argument)
                    .is_some_and(|ty| self.context.is_parameter_spec(ty))
                {
                    self.check_callable_annotation(argument);
                }
            }
            return true;
        }
        if name == "Concatenate" {
            self.emit(
                "valid-type",
                "The last parameter to Concatenate needs to be a ParamSpec".into(),
                expression,
            );
            self.check_callable_annotation(&subscript.slice);
            return true;
        }
        if name != "Callable" {
            return false;
        }
        let Expr::Tuple(tuple) = subscript.slice.as_ref() else {
            return false;
        };
        let [parameters, returns] = tuple.elts.as_slice() else {
            return false;
        };
        match parameters {
            Expr::List(list) => {
                let previous = std::mem::replace(&mut self.annotation_in_signature, true);
                let previous_errors = (!previous).then(|| std::mem::take(&mut self.parameter_spec_errors));
                for parameter in &list.elts { self.check_callable_annotation(parameter); }
                self.annotation_in_signature = previous;
                if let Some(previous_errors) = previous_errors { self.parameter_spec_errors = previous_errors; }
            }
            Expr::EllipsisLiteral(_) => {},
            Expr::Subscript(concatenate) if self.context.typing_form(&concatenate.value, &variables).as_deref() == Some("Concatenate") => {
                let arguments: Vec<_> = match concatenate.slice.as_ref() { Expr::Tuple(tuple) => tuple.elts.iter().collect(), expression => vec![expression] };
                let Some((last, prefix)) = arguments.split_last() else { return true; };
                if !last.is_ellipsis_literal_expr() && !self.scopes.annotation(last).is_some_and(|ty| self.context.is_parameter_spec(ty)) {
                    self.emit("valid-type", "The last parameter to Concatenate needs to be a ParamSpec".into(), parameters);
                }
                for argument in prefix { self.check_callable_annotation(argument); }
            }
            parameter if self.scopes.annotation(parameter).is_some_and(|ty| self.context.is_parameter_spec(ty)) => {},
            _ => self.emit_with_hint("valid-type", "The first argument to Callable must be a list of types, parameter specification, or \"...\"".into(), expression, Some("See https://mypy.readthedocs.io/en/stable/kinds_of_types.html#callable-types-and-lambdas".into())),
        }
        self.check_callable_annotation(returns);
        true
    }

    pub(super) fn check_parameter_spec_function(&mut self, function: &StmtFunctionDef) {
        let projection = |annotation: Option<&Expr>, args: bool| {
            annotation
                .and_then(|annotation| self.scopes.annotation(annotation))
                .and_then(|ty| self.context.node(ty))
                .and_then(|node| match node.as_ref() {
                    types::Node::ParamSpecArgs(variable)
                        if args && self.parameter_spec_bound(*variable) =>
                    {
                        Some(*variable)
                    }
                    types::Node::ParamSpecKwargs(variable)
                        if !args && self.parameter_spec_bound(*variable) =>
                    {
                        Some(*variable)
                    }
                    _ => None,
                })
        };
        let args = projection(
            function
                .parameters
                .vararg
                .as_ref()
                .and_then(|parameter| parameter.annotation.as_deref()),
            true,
        );
        let kwargs = projection(
            function
                .parameters
                .kwarg
                .as_ref()
                .and_then(|parameter| parameter.annotation.as_deref()),
            false,
        );
        for (parameter, positional) in [
            (function.parameters.vararg.as_ref(), true),
            (function.parameters.kwarg.as_ref(), false),
        ] {
            let Some(annotation) = parameter.and_then(|parameter| parameter.annotation.as_deref())
            else {
                continue;
            };
            let Some(node) = self
                .scopes
                .annotation(annotation)
                .and_then(|ty| self.context.node(ty))
            else {
                continue;
            };
            let wrong = match node.as_ref() {
                types::Node::ParamSpecKwargs(variable) if positional => Some(*variable),
                types::Node::ParamSpecArgs(variable) if !positional => Some(*variable),
                _ => None,
            };
            if let Some(variable) = wrong.filter(|&variable| self.parameter_spec_bound(variable)) {
                let name = self.context.name(variable);
                self.emit(
                    "valid-type",
                    if positional {
                        format!("Use \"{name}.args\" for variadic \"*\" parameter")
                    } else {
                        format!("Use \"{name}.kwargs\" for variadic \"**\" parameter")
                    },
                    annotation,
                );
            }
        }
        let point =
            ruff_text_size::TextRange::at(function.start(), ruff_text_size::TextSize::new(1));
        if args.is_some() && !function.parameters.kwonlyargs.is_empty() {
            self.emit(
                "valid-type",
                "Arguments not allowed after ParamSpec.args".into(),
                &point,
            );
        } else if args != kwargs {
            let variable = args.or(kwargs).unwrap();
            let name = self.context.name(variable);
            self.emit("valid-type", format!("ParamSpec must have \"*args\" typed as \"{name}.args\" and \"**kwargs\" typed as \"{name}.kwargs\""), &point);
        }
    }

    pub(super) fn check_parameter_spec_legacy(&mut self, assignment: &StmtAssign) {
        let [Expr::Name(name)] = assignment.targets.as_slice() else {
            return;
        };
        let Expr::Call(call) = assignment.value.as_ref() else {
            return;
        };
        if self
            .context
            .typing_form(&call.func, &self.scopes.variables())
            .as_deref()
            != Some("ParamSpec")
        {
            return;
        }
        match call.arguments.args.first() {
            Some(Expr::StringLiteral(string)) if string.value.to_str() != name.id.as_str() => self.emit("misc", format!("String argument 1 \"{}\" to ParamSpec(...) does not match variable name \"{}\"", string.value.to_str(), name.id), assignment),
            Some(Expr::StringLiteral(_)) => {},
            value => {
                self.emit("misc", "ParamSpec() expects a string literal as first argument".into(), assignment);
                if let Some(value) = value {
                    if let Some(actual) = self.infer(value) {
                        if actual != types::Type::Str { self.emit("arg-type", format!("Argument 1 to \"ParamSpec\" has incompatible type \"{}\"; expected \"str\"", self.context.name(actual)), value); }
                    }
                }
            }
        }
        for keyword in &call.arguments.keywords {
            match keyword.arg.as_ref().map(|name| name.as_str()) {
                Some("bound" | "covariant" | "contravariant") => self.emit("misc", "The variance and bound arguments to ParamSpec do not have defined semantics yet".into(), assignment),
                Some("default") => self.check_parameter_spec_default(&keyword.value),
                _ => {},
            }
        }
    }

    fn check_parameter_spec_default(&mut self, expression: &Expr) {
        if !matches!(expression, Expr::List(_) | Expr::EllipsisLiteral(_))
            && !self
                .scopes
                .annotation(expression)
                .is_some_and(|ty| ty.is_any() || self.context.is_parameter_spec(ty))
        {
            self.emit("misc", "The default argument to ParamSpec must be a list expression, ellipsis, or a ParamSpec".into(), expression);
        }
    }

    pub(super) fn check_parameter_spec_defaults(&mut self, parameters: Option<&TypeParams>) {
        for parameter in parameters
            .into_iter()
            .flat_map(|parameters| &parameters.type_params)
        {
            if let TypeParam::ParamSpec(parameter) = parameter {
                if let Some(default) = parameter.default.as_deref() {
                    self.check_parameter_spec_default(default);
                }
            }
        }
    }
}
