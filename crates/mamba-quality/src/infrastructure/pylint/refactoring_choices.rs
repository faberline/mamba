//! And-or suggestions retain their statement span and inferred truth boundary.
use super::{display, operand_inference::Operand, resolution::Context, values::Value, Checker};
use mamba_language::application::quality_syntax::python_ast::{BoolOp, Expr, Number, UnaryOp};
use ruff_text_size::TextRange;

impl Checker<'_> {
    fn choice_truth(&self, expression: &Expr) -> Option<bool> {
        if let Expr::UnaryOp(value) = expression {
            if matches!(value.op, UnaryOp::UAdd | UnaryOp::USub) {
                return self.choice_truth(&value.operand);
            }
        }
        let value = self.static_value(expression);
        match value {
            Value::Boolean(value) => return Some(value),
            Value::Number(Number::Int(value)) => return Some(value.to_string() != "0"),
            Value::Number(Number::Float(value)) => return Some(value != 0.0),
            Value::Number(Number::Complex { real, imag }) => {
                return Some(real != 0.0 || imag != 0.0)
            }
            Value::String(value) => return Some(!value.is_empty()),
            Value::Bytes(value) => return Some(!value.is_empty()),
            Value::Sequence(items, _) => return Some(!items.is_empty()),
            Value::Mapping(items) => return Some(!items.is_empty()),
            Value::Scalar("NoneType") => return Some(false),
            _ => {}
        }
        if let Some(value) =
            self.unpacked_value(expression, Context::Scope(self.scopes.current_index()))
        {
            if let Some(items) = value.elements {
                return Some(!items.is_empty());
            }
            if let Some(items) = value.entries {
                return Some(!items.is_empty());
            }
        }
        if let Expr::Call(call) = expression {
            if call.arguments.args.is_empty()
                && call.arguments.keywords.is_empty()
                && matches!(
                    self.stdlib_name(&call.func).as_deref(),
                    Some(
                        "builtins.int"
                            | "builtins.float"
                            | "builtins.complex"
                            | "builtins.bool"
                            | "builtins.str"
                            | "builtins.bytes"
                    )
                )
            {
                return Some(false);
            }
        }
        match self.operand(expression) {
            Operand::Function(_) | Operand::Class(_, _) => Some(true),
            Operand::Instance(class, _)
                if class.identity.0 == "builtins" && class.name == "object" =>
            {
                Some(true)
            }
            _ => None,
        }
    }
    pub(super) fn check_choice(&mut self, value: &Expr, range: TextRange) {
        let Expr::BoolOp(outer) = value else {
            return;
        };
        let [Expr::BoolOp(inner), fallback] = outer.values.as_slice() else {
            return;
        };
        let [condition, truth] = inner.values.as_slice() else {
            return;
        };
        if outer.op != BoolOp::Or
            || inner.op != BoolOp::And
            || matches!(truth, Expr::BoolOp(_))
            || matches!(fallback, Expr::BoolOp(_))
            || (matches!(truth, Expr::Compare(_)) && matches!(fallback, Expr::Compare(_)))
        {
            return;
        }
        let Some(truthy) = self.choice_truth(truth) else {
            return;
        };
        let fallback = display::expression(fallback, &self.source.text);
        if truthy {
            let suggestion = format!(
                "{} if {} else {fallback}",
                display::expression(truth, &self.source.text),
                display::expression(condition, &self.source.text)
            );
            self.emit(
                "R1706",
                "consider-using-ternary",
                &format!("Consider using ternary ({suggestion})"),
                range,
            );
        } else {
            self.emit(
                "R1709",
                "simplify-boolean-expression",
                &format!("Boolean expression may be simplified to {fallback}"),
                range,
            );
        }
    }
}
