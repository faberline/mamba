//! Decorator diagnostics retain semantic property and erased callable behavior.
use super::*;

fn last_name(expression: &Expr) -> Option<&str> {
    match expression {
        Expr::Name(name) => Some(name.id.as_str()),
        Expr::Attribute(attribute) => Some(attribute.attr.as_str()),
        _ => None,
    }
}

impl Checker<'_> {
    pub(super) fn check_property_decorators(&mut self, function: &StmtFunctionDef) {
        let mut nontrivial = false;
        let mut property = false;
        for decorator in &function.decorator_list {
            let name = last_name(&decorator.expression);
            let cached = self
                .context
                .annotation_binding(&decorator.expression, &self.scopes.variables())
                .and_then(|ty| self.context.class_id(ty))
                .is_some_and(|id| self.context.class(id).namespace == "functools.cached_property");
            if cached || name == Some("property") || name == Some("abstractproperty") {
                property = true;
            } else if !property
                && !matches!(
                    name,
                    Some(
                        "staticmethod"
                            | "classmethod"
                            | "abstractmethod"
                            | "override"
                            | "final"
                            | "overload"
                    )
                )
            {
                nontrivial = true;
            }
        }
        if property && nontrivial {
            self.emit(
                "prop-decorator",
                "Decorators on top of @property are not supported".into(),
                &ruff_text_size::TextRange::new(
                    function.decorator_list[0].expression.start(),
                    function.end(),
                ),
            );
        }
    }

    fn bound_final_decorators(&mut self, function: &StmtFunctionDef) {
        for decorator in &function.decorator_list {
            let final_marker = self
                .infer(&decorator.expression)
                .and_then(|ty| self.context.callable_signature(ty))
                .is_some_and(|signature| signature.native_symbol == Some("typing.final"));
            if !final_marker {
                continue;
            }
            if self.in_class {
                if let Some(id) = self
                    .class_types
                    .last()
                    .copied()
                    .flatten()
                    .and_then(|ty| self.context.class_id(ty))
                {
                    let mut class = (*self.context.class(id)).clone();
                    if let Some(method) = class.methods.get_mut(function.name.as_str()) {
                        method.final_method = true;
                        self.context.replace_class(id, class);
                    }
                }
            } else {
                self.emit(
                    "misc",
                    "@final cannot be used with non-method functions".into(),
                    &decorator.expression,
                );
            }
        }
    }

    fn bound_property_setter(&self, function: &StmtFunctionDef, expression: &Expr) -> bool {
        let Expr::Attribute(attribute) = expression else {
            return false;
        };
        let Expr::Name(receiver) = attribute.value.as_ref() else {
            return false;
        };
        if !self.in_class
            || attribute.attr.as_str() != "setter"
            || receiver.id.as_str() != function.name.as_str()
            || self.scopes.binding_location(receiver.id.as_str()) != Some(function.name.start())
        {
            return false;
        }
        let Some(id) = self
            .class_types
            .last()
            .copied()
            .flatten()
            .and_then(|ty| self.context.class_id(ty))
        else {
            return false;
        };
        let class = self.context.class(id);
        let property = class
            .methods
            .get(receiver.id.as_str())
            .is_some_and(|method| {
                method.kind == super::super::super::classes::MethodKind::Property
                    && method.setter.is_some()
            });
        // The class scope binds this declaration as a callable. An Any receiver
        // or a different live callable must not inherit its descriptor identity.
        property
            && self
                .infer(&attribute.value)
                .and_then(|ty| self.context.callable_signature(ty))
                .zip(self.scopes.callable(receiver.id.as_str()))
                .is_some_and(|(live, declared)| live.as_ref() == declared.as_ref())
    }

    pub(super) fn check_untyped_decorators<'a>(&mut self, function: &'a StmtFunctionDef)
    where
        Self: Visitor<'a>,
    {
        self.bound_final_decorators(function);
        // Untyped-decorator diagnostics still use each decorator independently.
        let annotated = function.returns.is_some()
            || function
                .parameters
                .iter()
                .any(|parameter| parameter.annotation().is_some());
        let applications = self.scopes.function_decorator_application(function);
        let environment = self.environment();
        for decorator in function.decorator_list.iter().rev() {
            if let Expr::Call(call) = &decorator.expression {
                // Real factory operands need diagnostics even when the result is Any.
                // Outer factory/application checks keep their existing owners.
                for argument in &call.arguments.args {
                    self.visit_expr(argument);
                }
                for keyword in &call.arguments.keywords {
                    self.visit_expr(&keyword.value);
                }
            }
            if self.bound_property_setter(function, &decorator.expression) {
                continue;
            }
            // computed_field factories have their own overload diagnostics and
            // erased-result check. Do not emit the same error a second time.
            if matches!(&decorator.expression, Expr::Call(call) if self
                .scopes
                .callable_expression_in(&call.func, &environment)
                .is_some_and(|signature| signature.native_symbol == Some("pydantic.fields.computed_field")))
            {
                continue;
            }
            // The expression's Any type takes precedence over a callable
            // remembered from an assignment to that expression's variable.
            let untyped = self
                .infer(&decorator.expression)
                .is_some_and(types::Type::is_any)
                || self
                    .scopes
                    .callable_expression_in(&decorator.expression, &environment)
                    .is_some_and(|signature| {
                        if signature.overloads.is_empty() {
                            signature.implicit || signature.ignored
                        } else {
                            signature
                                .overloads
                                .iter()
                                .any(|variant| variant.implicit || variant.ignored)
                        }
                    });
            if annotated && untyped {
                self.emit(
                    "untyped-decorator",
                    format!(
                        "Untyped decorator makes function \"{}\" untyped",
                        function.name
                    ),
                    &decorator.expression,
                );
            }
            if let Some((call, argument, signature)) = applications
                .calls
                .iter()
                .find(|(call, _, _)| call.func.range() == decorator.expression.range())
            {
                let previous = self
                    .flow
                    .narrowed
                    .insert(scopes::Scopes::DECORATOR_ARGUMENT.into(), *argument);
                if signature.overloads.is_empty() {
                    self.bind_call(call, signature);
                } else {
                    self.check_overload_call(call, signature);
                }
                if let Some(previous) = previous {
                    self.flow
                        .narrowed
                        .insert(scopes::Scopes::DECORATOR_ARGUMENT.into(), previous);
                } else {
                    self.flow
                        .narrowed
                        .remove(scopes::Scopes::DECORATOR_ARGUMENT);
                }
            }
        }
    }

    pub(super) fn check_computed_decorators(&mut self, function: &StmtFunctionDef) {
        for decorator in &function.decorator_list {
            let Expr::Call(call) = &decorator.expression else {
                continue;
            };
            let Some(signature) = self
                .scopes
                .callable_expression_in(&call.func, &self.environment())
            else {
                continue;
            };
            if signature.native_symbol != Some("pydantic.fields.computed_field") {
                continue;
            }
            self.check_overload_call(call, &signature);
            if self.scopes.invalid_computed_decorator(call) {
                self.emit(
                    "untyped-decorator",
                    format!(
                        "Untyped decorator makes function \"{}\" untyped",
                        function.name
                    ),
                    call,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::{ports::ProjectStore, source::PythonVersion};
    use crate::infrastructure::filesystem::Filesystem;

    #[test]
    fn factory_operands_visit_implicit_calls_once_and_keep_body_errors() {
        let source = r#"from typing import Any, Callable

def value():
    return "tag"

def typed_factory(label: str, *, extra: str) -> Callable[[Callable[[], None]], Callable[[], None]]:
    def decorate(fn: Callable[[], None]) -> Callable[[], None]:
        return fn
    return decorate

any_factory: Any = typed_factory

@typed_factory(
    value(),
    extra=value(),
)
def typed_target() -> None:
    print(body_missing_sentinel)

@any_factory(
    value(),
    extra=value(),
)
def any_target() -> None:
    pass
"#;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("main.py");
        std::fs::write(&path, source).unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        workspace.projects[0].python_version = PythonVersion::Py312;
        let report = crate::infrastructure::mypy::check(&workspace);
        let calls: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.rule == "no-untyped-call")
            .map(|diagnostic| (diagnostic.location.line, diagnostic.message.as_str()))
            .collect();
        assert_eq!(
            calls,
            vec![
                (14, "Call to untyped function \"value\" in typed context"),
                (15, "Call to untyped function \"value\" in typed context"),
                (21, "Call to untyped function \"value\" in typed context"),
                (22, "Call to untyped function \"value\" in typed context"),
            ]
        );
        let undefined: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.rule == "name-defined")
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();
        assert_eq!(undefined, ["Name \"body_missing_sentinel\" is not defined"]);
        assert_eq!(std::fs::read(&path).unwrap(), source.as_bytes());
    }
}
