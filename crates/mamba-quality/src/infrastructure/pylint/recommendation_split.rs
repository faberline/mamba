//! Split recommendations retain explicit and inferred keyword spelling.
use super::{display, operand_inference::Operand, resolution::Context, values::Value, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, ExprCall, Number, UnaryOp};
use ruff_text_size::Ranged;

fn numeric(value: &Expr) -> Option<f64> {
    match value {
        Expr::NumberLiteral(value) => match &value.value {
            Number::Int(value) => value.to_string().parse().ok(),
            Number::Float(value) => Some(*value),
            _ => None,
        },
        Expr::BooleanLiteral(value) => Some(f64::from(u8::from(value.value))),
        Expr::UnaryOp(value) if value.op == UnaryOp::USub => {
            numeric(&value.operand).map(|value| -value)
        }
        _ => None,
    }
}
impl Checker<'_> {
    fn recommendation_keyword(&self, call: &ExprCall, position: usize, name: &str) -> Option<Expr> {
        if let Some(value) = call.arguments.args.get(position) {
            return Some(value.clone());
        }
        if let Some(value) = call.arguments.keywords.iter().find(|value| {
            value
                .arg
                .as_ref()
                .is_some_and(|value| value.as_str() == name)
        }) {
            return Some(value.value.clone());
        }
        for keyword in &call.arguments.keywords {
            if keyword.arg.is_some() {
                continue;
            }
            let Some(value) =
                self.unpacked_value(&keyword.value, Context::Scope(self.scopes.current_index()))
            else {
                continue;
            };
            if let Some(entries) = value.entries {
                if let Some((_, value)) = entries.into_iter().find(|(key, _)| matches!(self.static_value(key), Value::String(value) if value == name)) { return Some(value); }
            }
        }
        None
    }
    pub(super) fn check_split_recommendation(&mut self, call: &ExprCall) {
        let Expr::Attribute(attribute) = call.func.as_ref() else {
            return;
        };
        if !matches!(attribute.attr.as_str(), "split" | "rsplit")
            || !matches!(
                self.operand(&attribute.value),
                Operand::Literal("str" | "bytes")
            )
        {
            return;
        }
        let Some(slice) = self.split_subscripts.get(&call.start().to_u32()) else {
            return;
        };
        let values = self.slot_nodes(slice, Context::Scope(self.scopes.current_index()));
        let Some((slice, origin)) = values.first().filter(|_| values.len() == 1) else {
            return;
        };
        let Some(index) = numeric(slice)
            .or_else(|| {
                self.unpacking_integer(slice, origin.clone())
                    .map(|value| value as f64)
            })
            .filter(|value| *value == -1.0 || *value == 0.0)
        else {
            return;
        };
        let Some(separator) = self.recommendation_keyword(call, 0, "sep") else {
            return;
        };
        if self.recommendation_keyword(call, 1, "maxsplit").is_some() {
            return;
        }
        let function = if index == -1.0 { "rsplit" } else { "split" };
        let index = if matches!(slice, Expr::BinOp(_)) {
            format!("{index:.0}")
        } else {
            display::expression(slice, &self.source.text)
        };
        let name = display::expression(&attribute.value, &self.source.text);
        let separator = display::expression(&separator, &self.source.text);
        self.emit(
            "C0207",
            "use-maxsplit-arg",
            &format!("Use {name}.{function}({separator}, maxsplit=1)[{index}] instead"),
            call.range(),
        );
    }
}
