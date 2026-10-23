//! Result inference distinguishes unknown values, builtin subtypes, and iterators.
use super::super::{
    callables::Target, classes::Class, resolution::Context, values::Value, Checker,
};
use mamba_language::application::quality_syntax::python_ast::{Expr, UnaryOp};
use std::rc::Rc;

pub(super) enum ResultValue {
    Unknown,
    Value(Value),
    Instance(Rc<Class>, Context),
    Class(Rc<Class>, Context),
    Iterator,
    Other,
}

impl Checker<'_> {
    pub(super) fn protocol_result(&self, expression: &Expr) -> ResultValue {
        match expression {
            Expr::Generator(_) | Expr::ListComp(_) | Expr::SetComp(_) | Expr::DictComp(_) => {
                return ResultValue::Unknown
            }
            Expr::UnaryOp(value) if matches!(value.op, UnaryOp::USub | UnaryOp::UAdd) => {
                if let Expr::NumberLiteral(value) = value.operand.as_ref() {
                    return ResultValue::Value(Value::Number(value.value.clone()));
                }
            }
            Expr::Call(call) => {
                if matches!(call.func.as_ref(), Expr::Name(value) if value.id.as_str() == "iter" && !self.scopes.declared("iter"))
                {
                    return ResultValue::Unknown;
                }
                if call.arguments.is_empty()
                    && self.expression_target(&call.func).is_some_and(|value| matches!(value.target, Target::Class(class) if class.identity.0 == "builtins" && class.name == "tuple")) {
                    return ResultValue::Value(Value::Sequence(Vec::new(), "tuple"));
                }
            }
            _ => {}
        }
        match self.expression_target(expression) {
            Some(resolved) => match resolved.target {
                Target::Value(value) => match value.as_ref() {
                    Value::Unknown => ResultValue::Unknown,
                    value => ResultValue::Value(self.expand_value(
                        value.clone(),
                        &resolved.context,
                        &mut std::collections::BTreeSet::new(),
                    )),
                },
                Target::Instance(class, origin) => ResultValue::Instance(class, origin),
                Target::Class(class) => ResultValue::Class(class, resolved.context),
                _ => ResultValue::Other,
            },
            None => ResultValue::Unknown,
        }
    }
    pub(super) fn protocol_accepts(&self, value: &ResultValue, expected: &str) -> bool {
        match value {
            ResultValue::Value(value) => {
                let name = match value {
                    Value::Sequence(_, name) => Some(*name),
                    Value::Mapping(_) => Some("dict"),
                    value => value.builtin_name(),
                };
                name == Some(expected) || (expected == "int" && name == Some("bool"))
            }
            ResultValue::Instance(class, origin) => {
                if expected == "iterator" {
                    return self.class_member(class.clone(), origin.clone(), "__next__", true).is_some();
                }
                self.lineage(class.clone(), origin.clone()).iter().any(|(class, _)| class.identity.0 == "builtins" && class.name == expected)
            }
            ResultValue::Class(class, origin) if expected == "iterator" => {
                class.metaclass.as_ref().and_then(|value| self.expression_target_in(value, origin.clone())).is_some_and(|value| matches!(value.target, Target::Class(class) if self.class_member(class.clone(), value.context, "__next__", false).is_some()))
            }
            ResultValue::Iterator => expected == "iterator",
            _ => false,
        }
    }
}
