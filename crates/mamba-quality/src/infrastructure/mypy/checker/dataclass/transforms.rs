//! Transform parameter errors retain literal checks and normal call type checks.
use super::super::*;
use quality_syntax::python_ast::{Decorator, ExprCall, StmtClassDef, StmtFunctionDef};

fn transform_flag(name: &str) -> bool {
    matches!(
        name,
        "eq_default" | "order_default" | "kw_only_default" | "frozen_default"
    )
}

impl Checker<'_> {
    pub(in crate::infrastructure::mypy::checker) fn check_dataclass_transform_function(
        &mut self,
        function: &StmtFunctionDef,
    ) {
        self.check_dataclass_transform_decorators(&function.decorator_list);
    }

    pub(in crate::infrastructure::mypy::checker) fn check_dataclass_transform_class(
        &mut self,
        class: &StmtClassDef,
    ) {
        self.check_dataclass_transform_decorators(&class.decorator_list);
    }

    fn check_dataclass_transform_decorators(&mut self, decorators: &[Decorator]) {
        for decorator in decorators {
            let Expr::Call(call) = &decorator.expression else {
                continue;
            };
            if self
                .context
                .typing_form(&call.func, &self.scopes.variables())
                .as_deref()
                != Some("dataclass_transform")
            {
                continue;
            }
            for keyword in &call.arguments.keywords {
                let Some(name) = keyword.arg.as_ref().map(|name| name.as_str()) else {
                    continue;
                };
                if name == "field_specifiers" {
                    self.check_transform_field_specifiers(&keyword.value);
                } else {
                    self.check_transform_boolean(name, &keyword.value);
                    if matches!(keyword.value, Expr::BooleanLiteral(_)) && !transform_flag(name) {
                        self.emit(
                            "misc",
                            format!("Unrecognized dataclass_transform parameter \"{name}\""),
                            call,
                        );
                    }
                    if transform_flag(name) {
                        if let Some(actual) = self
                            .infer(&keyword.value)
                            .filter(|&ty| !self.context.accepts(types::Type::Bool, ty))
                        {
                            self.emit("arg-type", format!("Argument \"{name}\" to \"dataclass_transform\" has incompatible type \"{}\"; expected \"bool\"", self.context.name(actual)), &keyword.value);
                        }
                    }
                }
            }
        }
    }

    fn check_transform_boolean(&mut self, name: &str, expression: &Expr) {
        if !matches!(expression, Expr::BooleanLiteral(_)) {
            self.emit(
                "literal-required",
                format!("\"{name}\" argument must be a True or False literal"),
                expression,
            );
        }
    }

    fn check_transform_field_specifiers(&mut self, expression: &Expr) {
        if let Expr::Tuple(tuple) = expression {
            if let Some(item) = tuple
                .elts
                .iter()
                .find(|item| !matches!(item, Expr::Name(_) | Expr::Attribute(_)))
            {
                self.emit(
                    "misc",
                    "\"field_specifiers\" must only contain identifiers".into(),
                    item,
                );
            }
        } else {
            self.emit(
                "misc",
                "\"field_specifiers\" argument must be a tuple literal".into(),
                expression,
            );
        }
        if let Some(actual) = self
            .infer(expression)
            .filter(|&ty| !self.transform_specifier_tuple(ty))
        {
            self.emit("arg-type", format!("Argument \"field_specifiers\" to \"dataclass_transform\" has incompatible type \"{}\"; expected \"tuple[type[Any] | Callable[..., Any], ...]\"", self.context.name(actual)), expression);
        }
    }

    fn transform_specifier_tuple(&self, ty: types::Type) -> bool {
        if ty.is_any() {
            return true;
        }
        match self.context.node(ty).as_deref() {
            Some(types::Node::Tuple(items)) => items.iter().all(|&ty| self.transform_specifier(ty)),
            Some(types::Node::VariadicTuple(item)) => self.transform_specifier(*item),
            _ => false,
        }
    }

    fn transform_specifier(&self, ty: types::Type) -> bool {
        ty.is_any()
            || self.context.is_class_value(ty)
            || matches!(
                self.context.node(ty).as_deref(),
                Some(types::Node::Callable(_))
            )
    }

    pub(in crate::infrastructure::mypy::checker) fn check_dataclass_transform_options(
        &mut self,
        class: &StmtClassDef,
        definition: &super::super::super::classes::dataclass::Definition,
    ) {
        if definition.transformed {
            if definition.class_options {
                if let Some(arguments) = class.arguments.as_deref() {
                    for keyword in &arguments.keywords {
                        let Some(name) = keyword.arg.as_ref().map(|name| name.as_str()) else {
                            continue;
                        };
                        if matches!(
                            name,
                            "eq" | "order" | "kw_only" | "frozen" | "init" | "slots" | "match_args"
                        ) {
                            self.check_transform_boolean(name, &keyword.value);
                        }
                    }
                }
            }
            for decorator in &class.decorator_list {
                let Expr::Call(call) = &decorator.expression else {
                    continue;
                };
                if !self
                    .context
                    .typing_form(&call.func, &self.scopes.variables())
                    .is_some_and(|form| form.starts_with("dataclass-transform:"))
                {
                    continue;
                }
                if let Some(signature) = self
                    .scopes
                    .callable_expression_in(&call.func, &self.environment())
                {
                    for keyword in &call.arguments.keywords {
                        let Some(name) = keyword.arg.as_ref().map(|name| name.as_str()) else {
                            continue;
                        };
                        if matches!(
                            name,
                            "eq" | "order" | "kw_only" | "frozen" | "init" | "slots" | "match_args"
                        ) && signature
                            .parameters
                            .iter()
                            .any(|parameter| parameter.name == name)
                        {
                            self.check_transform_boolean(name, &keyword.value);
                        }
                    }
                }
                self.check_function_call(&decorator.expression);
            }
        }
        for statement in &class.body {
            let Stmt::AnnAssign(assignment) = statement else {
                continue;
            };
            let Some(Expr::Call(call)) = assignment.value.as_deref() else {
                continue;
            };
            if !self
                .context
                .typing_form(&call.func, &self.scopes.variables())
                .is_some_and(|form| definition.field_specifiers.contains(&form))
            {
                continue;
            }
            self.check_transform_field_parameters(call);
        }
    }

    fn check_transform_field_parameters(&mut self, call: &ExprCall) {
        for keyword in &call.arguments.keywords {
            match keyword.arg.as_ref().map(|name| name.as_str()) {
                Some("alias") if !matches!(keyword.value, Expr::StringLiteral(_)) => self.emit(
                    "literal-required",
                    "\"alias\" argument to dataclass field must be a string literal".into(),
                    call,
                ),
                Some("kw_only") if !matches!(keyword.value, Expr::BooleanLiteral(_)) => self.emit(
                    "misc",
                    "\"kw_only\" argument must be a boolean literal".into(),
                    call,
                ),
                _ => {}
            }
        }
    }
}
