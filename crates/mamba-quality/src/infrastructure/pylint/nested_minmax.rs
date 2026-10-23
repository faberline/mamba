//! Nested builtin calls preserve original parent sizes and suggestion scan order.
use super::{display, operand_inference::Operand, resolution::Context, Checker};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, Visitor},
    Expr, ExprCall, Operator,
};
use ruff_text_size::Ranged;
use std::collections::BTreeMap;

#[derive(Default)]
struct Parents(BTreeMap<u32, usize>);
impl<'a> Visitor<'a> for Parents {
    fn visit_expr(&mut self, expression: &'a Expr) {
        if let Expr::Call(call) = expression {
            for argument in &call.arguments.args {
                if let Expr::Call(inner) = argument {
                    self.0
                        .insert(inner.start().to_u32(), call.arguments.args.len());
                }
            }
        }
        walk_expr(self, expression);
    }
}
impl Checker<'_> {
    fn minmax_name(&self, value: &Expr) -> Option<String> {
        self.stdlib_name(value)
            .filter(|name| matches!(name.as_str(), "builtins.min" | "builtins.max"))
    }
    fn minmax_splattable(&self, value: &Expr) -> bool {
        if let Expr::BinOp(binary) = value {
            if matches!(binary.op, Operator::Add | Operator::BitOr) {
                return self.minmax_splattable(&binary.left)
                    && self.minmax_splattable(&binary.right);
            }
        }
        if matches!(
            value,
            Expr::List(_)
                | Expr::Tuple(_)
                | Expr::Set(_)
                | Expr::Dict(_)
                | Expr::ListComp(_)
                | Expr::DictComp(_)
        ) {
            return true;
        }
        let operand = self.operand(value);
        if matches!(operand, Operand::Literal("list" | "tuple"))
            || matches!(operand, Operand::Instance(ref class, _) if class.identity.0 == "builtins" && matches!(class.name.as_str(), "list" | "tuple"))
        {
            return true;
        }
        self.unpacked_value(value, Context::Scope(self.scopes.current_index()))
            .is_some_and(|value| {
                value.view.is_some()
                    || value.entries.is_some()
                    || (value.elements.is_some() && value.kind == "set")
            })
    }
    pub(super) fn check_nested_minmax(&mut self, call: &ExprCall) {
        let Some(name) = self.minmax_name(&call.func) else {
            return;
        };
        let expression = Expr::Call(call.clone());
        let mut parents = Parents::default();
        parents.visit_expr(&expression);
        let redundant = |value: &Expr| {
            matches!(value, Expr::Call(inner) if parents.0.get(&inner.start().to_u32()).is_some_and(|count| *count > 1)
                && inner.arguments.keywords.is_empty()
                && self.minmax_name(&inner.func).as_ref() == Some(&name))
        };
        let mut arguments = call.arguments.args.to_vec();
        if !arguments.iter().any(&redundant) {
            return;
        }
        while arguments.iter().any(&redundant) {
            for (index, argument) in arguments.iter().enumerate() {
                if let Expr::Call(inner) = argument {
                    if inner
                        .arguments
                        .args
                        .iter()
                        .any(|argument| matches!(argument, Expr::Generator(_)))
                    {
                        return;
                    }
                    if redundant(argument) {
                        let replacement = inner.arguments.args.to_vec();
                        arguments.splice(index..=index, replacement);
                        break;
                    }
                }
            }
        }
        let mut arguments: Vec<_> = arguments
            .iter()
            .map(|argument| {
                let text = display::expression(argument, &self.source.text);
                if self.minmax_splattable(argument) {
                    format!("*{text}")
                } else {
                    text
                }
            })
            .collect();
        arguments.extend(call.arguments.keywords.iter().map(|keyword| {
            format!(
                "{}{}{}",
                keyword.arg.as_ref().map_or("**", |name| name.as_str()),
                if keyword.arg.is_some() { "=" } else { "" },
                display::expression(&keyword.value, &self.source.text)
            )
        }));
        let function = match call.func.as_ref() {
            Expr::Name(value) => value.id.as_str(),
            Expr::Attribute(value) => value.attr.as_str(),
            _ => return,
        };
        let suggestion = format!(
            "{}({})",
            display::expression(&call.func, &self.source.text),
            arguments.join(", ")
        );
        self.emit("W3301", "nested-min-max", &format!("Do not use nested call of '{function}'; it's possible to do '{suggestion}' instead"), call.range());
    }
}
