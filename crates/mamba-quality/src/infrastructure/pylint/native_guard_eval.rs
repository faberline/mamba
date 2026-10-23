//! False body guards retain constant-node identity rather than container truth alone.
use super::{callables::Target, values::Value, Checker};
use mamba_language::application::quality_syntax::python_ast::{
    BoolOp, CmpOp, Expr, ExprCompare, Number, UnaryOp,
};

impl Checker<'_> {
    pub(super) fn native_guard_constant(&self, expression: &Expr) -> Option<(bool, bool)> {
        if let Some(number) = super::simplifiable_numbers::number(expression) {
            return Some((number.truth(), true));
        }
        match self.static_value(expression) {
            Value::Boolean(value) => return Some((value, true)),
            Value::String(value) => return Some((!value.is_empty(), true)),
            Value::Bytes(value) => return Some((!value.is_empty(), true)),
            Value::Scalar("NoneType") => return Some((false, true)),
            Value::Number(Number::Int(value)) => return Some((value.to_string() != "0", true)),
            Value::Number(Number::Float(value)) => return Some((value != 0.0, true)),
            Value::Number(Number::Complex { real, imag }) => {
                return Some((real != 0.0 || imag != 0.0, true))
            }
            Value::Sequence(values, _) => return Some((!values.is_empty(), false)),
            Value::Mapping(values) => return Some((!values.is_empty(), false)),
            _ => {}
        }
        match expression {
            Expr::UnaryOp(value) if value.op == UnaryOp::Not => self
                .native_guard_constant(&value.operand)
                .map(|(truth, _)| (!truth, true)),
            Expr::BoolOp(value) => {
                let mut result = None;
                for child in &value.values {
                    let inferred = self.native_guard_constant(child)?;
                    result = Some(inferred);
                    if inferred.0 == (value.op == BoolOp::Or) {
                        break;
                    }
                }
                result
            }
            Expr::Compare(value) => self
                .simplification_truth(expression)
                .or_else(|| self.native_guard_string_comparison(value))
                .map(|truth| (truth, true)),
            Expr::If(value) => {
                let (truth, _) = self.native_guard_constant(&value.test)?;
                self.native_guard_constant(if truth { &value.body } else { &value.orelse })
            }
            Expr::Call(call) if matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "hasattr") =>
            {
                let [owner, name] = call.arguments.args.as_ref() else {
                    return None;
                };
                let Value::String(name) = self.static_value(name) else {
                    return None;
                };
                self.native_guard_has_attribute(owner, &name)
                    .map(|value| (value, true))
            }
            _ => None,
        }
    }

    fn native_guard_string_comparison(&self, comparison: &ExprCompare) -> Option<bool> {
        let mut left = self.native_guard_strings(comparison.operands.first()?)?;
        for (operator, right) in comparison
            .ops
            .iter()
            .zip(comparison.operands.iter().skip(1))
        {
            let right = self.native_guard_strings(right)?;
            let mut result = None;
            for left in &left {
                for right in &right {
                    let truth = match operator {
                        CmpOp::Eq => left == right,
                        CmpOp::NotEq => left != right,
                        CmpOp::Lt => left < right,
                        CmpOp::LtE => left <= right,
                        CmpOp::Gt => left > right,
                        CmpOp::GtE => left >= right,
                        CmpOp::In => right.contains(left),
                        CmpOp::NotIn => !right.contains(left),
                        _ => return None,
                    };
                    if result.is_some_and(|value| value != truth) {
                        return None;
                    }
                    result = Some(truth);
                }
            }
            if !result? {
                return Some(false);
            }
            left = right;
        }
        Some(true)
    }

    fn native_guard_strings(&self, expression: &Expr) -> Option<Vec<String>> {
        if let Value::String(value) = self.static_value(expression) {
            return Some(vec![value]);
        }
        let Expr::Attribute(attribute) = expression else {
            return None;
        };
        self.builtin_metadata_values(
            &attribute.value,
            super::resolution::Context::Scope(self.scopes.current_index()),
            attribute.attr.as_str(),
        )?
        .into_iter()
        .map(|value| match value.target {
            Target::Value(value) => match value.as_ref() {
                Value::String(value) => Some(value.clone()),
                _ => None,
            },
            _ => None,
        })
        .collect()
    }

    fn native_guard_has_attribute(&self, owner: &Expr, name: &str) -> Option<bool> {
        if let Some(module) = self.module_expression(owner) {
            return self
                .modules
                .exists(self.source.project, &module)
                .then(|| self.modules.member(self.source.project, &module, name));
        }
        let resolved = self.expression_target(owner)?;
        let (class, origin, instance) = match resolved.target {
            Target::Function(function) => {
                return self.source_function_member(&function, &resolved.context, name)
            }
            Target::Class(class) => (class, resolved.context, false),
            Target::Instance(class, origin) => (class, origin, true),
            Target::Value(value) => {
                let name = match value.as_ref() {
                    Value::Sequence(_, name) => Some(*name),
                    Value::Mapping(_) => Some("dict"),
                    value => value.builtin_name(),
                }?;
                (
                    self.modules.builtin_class(name)?,
                    super::resolution::Context::Module("builtins".into()),
                    true,
                )
            }
            _ => return None,
        };
        if self
            .class_member(class.clone(), origin.clone(), name, instance)
            .is_some()
            || self.source_class_scope_member(&class, name, instance)
            || (class.identity.0 == "builtins"
                && super::builtin_members::exists(&class.name, name, self.modules.target()))
        {
            return Some(true);
        }
        if class.metaclass.is_some()
            || self.dynamic_member(class.clone(), origin.clone())
            || self
                .lineage(class.clone(), origin)
                .iter()
                .any(|(class, context)| {
                    class
                        .bases
                        .iter()
                        .any(|base| self.inferred_base(base, context.clone()).is_none())
                })
        {
            return None;
        }
        Some(false)
    }

    pub(super) fn native_suppresses_attribute_error(&self, expression: &Expr) -> bool {
        let Expr::Call(call) = expression else {
            return false;
        };
        if !self.expression_target(&call.func).is_some_and(|resolved| {
            matches!(resolved.target, Target::Class(class) if class.identity.0 == "contextlib" && class.name == "suppress")
        }) {
            return false;
        }
        call.arguments.args.iter().any(|value| {
            let values = if let Expr::Tuple(value) = value {
                value.elts.as_slice()
            } else {
                std::slice::from_ref(value)
            };
            values.iter().any(|value| {
                self.expression_target(value).is_some_and(|resolved| {
                matches!(resolved.target, Target::Class(class) if class.name == "AttributeError")
            })
            })
        })
    }
}
