//! Comparison inference retains callable categories and original typecheck shapes.
use super::{
    callables::Target,
    resolution::{Context, Resolved},
    values::Value,
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::{
    Expr, ExprCall, Number, Operator, UnaryOp,
};
use ruff_text_size::Ranged;
use std::collections::BTreeSet;

type Active = BTreeSet<(Context, u32, u32)>;
impl Checker<'_> {
    fn comparison_target(
        &self,
        expression: &Expr,
        origin: Context,
        active: &mut Active,
    ) -> Option<Resolved> {
        let key = (
            origin.clone(),
            expression.start().to_u32(),
            expression.end().to_u32(),
        );
        if !active.insert(key.clone()) {
            return None;
        }
        let result = (|| {
            if let Expr::Call(call) = expression {
                let callee = self.expression_target_in(&call.func, origin.clone())?;
                if let Target::Function(function) = &callee.target {
                    if let Some(value) = &function.result_expression {
                        self.comparison_target(value, callee.context, active)
                    } else {
                        self.expression_target_in(expression, origin)
                    }
                } else {
                    self.expression_target_in(expression, origin)
                }
            } else {
                let value = self.expression_target_in(expression, origin)?;
                if let Target::Value(reference) = &value.target {
                    if let Value::Reference(names) = reference.as_ref() {
                        self.reference_in(names.clone(), value.context)
                    } else {
                        Some(value)
                    }
                } else {
                    Some(value)
                }
            }
        })();
        active.remove(&key);
        result
    }
    pub(super) fn comparison_callable(&self, expression: &Expr) -> bool {
        if let Expr::Name(name) = expression {
            if self.modules.function(name.id.as_str()) && !self.scopes.declared(name.id.as_str()) {
                return true;
            }
        }
        let Some(resolved) = self.comparison_target(
            expression,
            Context::Scope(self.scopes.current_index()),
            &mut Active::new(),
        ) else {
            return false;
        };
        let Target::Function(function) = resolved.target else {
            return false;
        };
        function.name != "<lambda>"
            && function.category != "unbound method"
            && !function.direct_raise
            && !function.decorators.iter().any(|value| {
                super::values::path(value).is_some_and(|names| names == ["typing", "_SpecialForm"])
            })
    }
    fn comparison_type_call<'a>(&self, expression: &'a Expr) -> Option<&'a ExprCall> {
        let Expr::Call(call) = expression else {
            return None;
        };
        if call.arguments.args.len() != 1 || !call.arguments.keywords.is_empty() {
            return None;
        }
        self.expression_target(&call.func).is_some_and(|value|matches!(value.target,Target::Class(class) if class.identity.0=="builtins" && class.name=="type")).then_some(call)
    }
    fn comparison_literal_inference(&self, expression: &Expr) -> bool {
        if let Some(resolved) = self.comparison_target(
            expression,
            Context::Scope(self.scopes.current_index()),
            &mut Active::new(),
        ) {
            if let Target::Value(value) = resolved.target {
                let value =
                    self.expand_value((*value).clone(), &resolved.context, &mut Default::default());
                if matches!(
                    value,
                    Value::String(_)
                        | Value::Bytes(_)
                        | Value::Number(_)
                        | Value::Boolean(_)
                        | Value::Scalar("NoneType")
                        | Value::Mapping(_)
                        | Value::Sequence(_, "list" | "set")
                ) {
                    return true;
                }
            }
        }
        if (matches!(expression,Expr::UnaryOp(value) if matches!(value.op,UnaryOp::UAdd | UnaryOp::USub | UnaryOp::Invert))
            || matches!(expression,Expr::BinOp(value) if value.op==Operator::Add))
            && matches!(
                self.slot_scalar(expression, Context::Scope(self.scopes.current_index())),
                Value::Number(_) | Value::Boolean(_) | Value::String(_)
            )
        {
            return true;
        }
        self.unpacked_value(expression, Context::Scope(self.scopes.current_index()))
            .is_some_and(|value| matches!(value.kind, "list" | "set" | "dict"))
    }
    pub(super) fn check_comparison_type(
        &mut self,
        left: &Expr,
        right: &Expr,
        range: ruff_text_size::TextRange,
    ) {
        let pair = if let Some(left) = self.comparison_type_call(left) {
            Some((left, right))
        } else if matches!(left, Expr::Name(_)) {
            self.comparison_type_call(right).map(|value| (value, left))
        } else {
            None
        };
        let Some((_, right)) = pair else {
            return;
        };
        if let Some(call) = self.comparison_type_call(right) {
            if !self.comparison_literal_inference(&call.arguments.args[0]) {
                return;
            }
        }
        self.emit(
            "C0123",
            "unidiomatic-typecheck",
            "Use isinstance() rather than type() for a typecheck.",
            range,
        );
    }
    pub(super) fn comparison_nan_constant(
        &self,
        expression: &Expr,
        origin: Context,
        active: &mut Active,
    ) -> bool {
        let key = (
            origin.clone(),
            expression.start().to_u32(),
            expression.end().to_u32(),
        );
        if !active.insert(key.clone()) {
            return false;
        }
        let result = match expression {
            Expr::NumberLiteral(value) => {
                matches!(value.value,Number::Float(value) if value.is_nan())
            }
            Expr::Name(name) => {
                let import = match &origin {
                    Context::Scope(index) => self
                        .scopes
                        .call_target_in(name.id.as_str(), *index)
                        .map(|(_, value)| value),
                    Context::Module(module) => {
                        self.modules
                            .call_target(self.source.project, module, name.id.as_str())
                    }
                };
                if matches!(import,Some(Target::Imported(import)) if import.module=="math" && import.member.as_deref()==Some("nan"))
                    && self
                        .modules
                        .source_index(self.source.project, "math")
                        .is_none()
                {
                    true
                } else if let Some((value, context)) =
                    self.source_binding(name.id.as_str(), &origin)
                {
                    self.comparison_nan_constant(&value, context, active)
                } else {
                    false
                }
            }
            Expr::Attribute(value) => match self.expression_target_in(&value.value, origin) {
                Some(Resolved {
                    target: Target::Module(module),
                    ..
                }) => {
                    if module == "math"
                        && value.attr.as_str() == "nan"
                        && self
                            .modules
                            .source_index(self.source.project, "math")
                            .is_none()
                    {
                        true
                    } else {
                        self.source_binding(value.attr.as_str(), &Context::Module(module))
                            .is_some_and(|(value, context)| {
                                self.comparison_nan_constant(&value, context, active)
                            })
                    }
                }
                _ => false,
            },
            _ => false,
        };
        active.remove(&key);
        result
    }
    pub(super) fn comparison_nan(&self, expression: &Expr) -> bool {
        let origin = Context::Scope(self.scopes.current_index());
        if matches!(expression,Expr::Attribute(value) if matches!(value.attr.as_str(),"nan" | "NaN" | "NAN") && matches!(value.value.as_ref(),Expr::Name(name) if matches!(name.id.as_str(),"np" | "numpy")))
        {
            return true;
        }
        if matches!(expression, Expr::Name(_) | Expr::Attribute(_)) {
            return self.comparison_nan_constant(expression, origin, &mut Active::new());
        }
        let Expr::Call(call) = expression else {
            return false;
        };
        let [Expr::StringLiteral(value)] = call.arguments.args.as_ref() else {
            return false;
        };
        if !value.value.to_str().eq_ignore_ascii_case("nan") {
            return false;
        }
        if matches!(call.func.as_ref(),Expr::Name(name) if name.id.as_str()=="Decimal")
            || matches!(call.func.as_ref(),Expr::Attribute(value) if value.attr.as_str()=="Decimal")
        {
            return true;
        }
        self.comparison_target(expression, origin, &mut Active::new())
            .is_some_and(|value| match value.target {
                Target::Instance(class, _) => {
                    class.identity.0 == "builtins" && class.name == "float"
                }
                Target::Value(value) => matches!(value.as_ref(), Value::Number(Number::Float(_))),
                _ => false,
            })
    }
}
