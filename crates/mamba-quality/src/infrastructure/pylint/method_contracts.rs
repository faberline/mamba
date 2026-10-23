//! Property signatures and overridden method kinds retain header diagnostics.
use super::{classes::MethodKind, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, StmtFunctionDef};
use ruff_text_size::Ranged;

fn accessor(function: &StmtFunctionDef, names: &[&str]) -> bool {
    function.decorator_list.iter().any(|value| matches!(&value.expression, Expr::Attribute(value) if names.contains(&value.attr.as_str())))
}

impl Checker<'_> {
    pub(super) fn check_property_parameters(
        &mut self,
        function: &StmtFunctionDef,
        kind: MethodKind,
    ) {
        let range = self.declaration_range(function.name.range());
        if kind == MethodKind::Property
            && function.parameters.iter().count() > 1
            && !accessor(function, &["setter"])
        {
            self.emit(
                "R0206",
                "property-with-parameters",
                "Cannot have defined parameters for properties",
                range,
            );
        }
    }
    pub(super) fn check_method_contract(&mut self, function: &StmtFunctionDef, kind: MethodKind) {
        let range = self.declaration_range(function.name.range());
        if function.name.as_str() == "__init__" {
            return;
        }
        let Some((class, context)) = self.class_stack.last().cloned() else {
            return;
        };
        let class_name = class.name.clone();
        let Ok(order) = self.mro(class, context) else {
            return;
        };
        for (parent, context) in order.into_iter().skip(1) {
            let Some(method) = parent.methods.get(function.name.as_str()) else {
                continue;
            };
            let parent_kind = self.inferred_method_kind(
                &method.signature.decorators,
                context.clone(),
                method.kind,
            );
            if parent.identity.0 != "builtins" {
                self.check_override_signature(
                    function,
                    kind,
                    &method.signature,
                    parent_kind,
                    &parent.name,
                    &class_name,
                );
            }
            let parent_property = parent_kind == MethodKind::Property || method.signature.decorators.iter().any(|value| matches!(value, Expr::Attribute(value) if matches!(value.attr.as_str(), "setter" | "deleter")));
            let current_property =
                kind == MethodKind::Property || accessor(function, &["setter", "deleter"]);
            if parent_property != current_property {
                let (before, after) = if current_property {
                    ("method", "property")
                } else {
                    (
                        "property",
                        match kind {
                            MethodKind::Class => "classmethod",
                            MethodKind::Static => "staticmethod",
                            _ => "method",
                        },
                    )
                };
                self.emit(
                    "W0236",
                    "invalid-overridden-method",
                    &format!(
                        "Method '{}' was expected to be '{before}', found it instead as '{after}'",
                        function.name
                    ),
                    range,
                );
            }
            if method.signature.is_async != function.is_async {
                let (before, after) = if function.is_async {
                    ("non-async", "async")
                } else {
                    ("async", "non-async")
                };
                self.emit(
                    "W0236",
                    "invalid-overridden-method",
                    &format!(
                        "Method '{}' was expected to be '{before}', found it instead as '{after}'",
                        function.name
                    ),
                    range,
                );
            }
            if method.signature.decorators.iter().any(|decorator| {
                matches!(
                    self.qualified_decorator(decorator, context.clone())
                        .as_deref(),
                    Some("typing.final" | "typing_extensions.final")
                )
            }) {
                self.emit("W0239", "overridden-final-method", &format!("Method '{}' overrides a method decorated with typing.final which is defined in class '{}'", function.name, parent.name), range);
            }
            break;
        }
    }
}
