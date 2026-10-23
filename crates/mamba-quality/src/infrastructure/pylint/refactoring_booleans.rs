//! Boolean refactoring preserves original syntax guards and deterministic text order.
use super::{display, operand_inference::Operand, resolution::Context, Checker};
use mamba_language::application::quality_syntax::python_ast::{BoolOp, CmpOp, Expr, UnaryOp};
use ruff_text_size::Ranged;
use std::collections::BTreeSet;

fn reverse(operator: CmpOp) -> CmpOp {
    match operator {
        CmpOp::Eq => CmpOp::NotEq,
        CmpOp::NotEq => CmpOp::Eq,
        CmpOp::Lt => CmpOp::GtE,
        CmpOp::GtE => CmpOp::Lt,
        CmpOp::LtE => CmpOp::Gt,
        CmpOp::Gt => CmpOp::LtE,
        CmpOp::Is => CmpOp::IsNot,
        CmpOp::IsNot => CmpOp::Is,
        CmpOp::In => CmpOp::NotIn,
        CmpOp::NotIn => CmpOp::In,
    }
}
impl Checker<'_> {
    fn negation_operand(&self, value: &Expr) -> bool {
        let operand = self.operand(value);
        let inferred = self.unpacked_value(value, Context::Scope(self.scopes.current_index()));
        if inferred
            .as_ref()
            .is_some_and(|value| value.view == Some("values"))
        {
            return true;
        }
        if matches!(
            operand,
            Operand::Unknown | Operand::Literal("set" | "frozenset" | "NoneType")
        ) {
            return false;
        }
        if matches!(operand, Operand::Instance(ref class, _) if class.identity.0 == "builtins" && matches!(class.name.as_str(), "set" | "frozenset"))
        {
            return false;
        }
        !inferred.is_some_and(|value| matches!(value.view, Some("keys" | "items")))
    }
    pub(super) fn check_refactoring_boolean(&mut self, value: &Expr) {
        if let Expr::UnaryOp(unary) = value {
            if let Expr::Compare(comparison) = unary.operand.as_ref() {
                if unary.op == UnaryOp::Not
                    && comparison.ops.len() == 1
                    && !(self.method_context.as_deref() == Some("__ne__")
                        && comparison.ops[0] == CmpOp::Eq)
                    && comparison
                        .operands
                        .iter()
                        .all(|value| self.negation_operand(value))
                {
                    let suggestion = format!(
                        "{} {} {}",
                        display::expression(&comparison.operands[0], &self.source.text),
                        reverse(comparison.ops[0]),
                        display::expression(&comparison.operands[1], &self.source.text)
                    );
                    self.emit(
                        "C0117",
                        "unnecessary-negation",
                        &format!(
                            "Consider changing \"{}\" to \"{suggestion}\"",
                            display::expression(value, &self.source.text)
                        ),
                        value.range(),
                    );
                }
            }
        }
        let Expr::BoolOp(boolean) = value else {
            return;
        };
        self.check_merging_isinstance(boolean);
        if boolean.values.len() < 2 {
            return;
        }
        let operator = if boolean.op == BoolOp::Or {
            CmpOp::Eq
        } else {
            CmpOp::NotEq
        };
        let mut common: Option<BTreeSet<String>> = None;
        let mut values = Vec::new();
        for value in &boolean.values {
            let Expr::Compare(comparison) = value else {
                return;
            };
            if comparison.ops.len() != 1
                || comparison.ops[0] != operator
                || comparison
                    .operands
                    .iter()
                    .any(|value| matches!(value, Expr::Call(_)))
            {
                return;
            }
            let mut variables = BTreeSet::new();
            for operand in &comparison.operands {
                let text = display::expression(operand, &self.source.text);
                if matches!(operand, Expr::Name(_) | Expr::Attribute(_)) {
                    variables.insert(text.clone());
                }
                if !values.contains(&text) {
                    values.push(text);
                }
            }
            common = Some(match common {
                None => variables,
                Some(common) => common.intersection(&variables).cloned().collect(),
            });
        }
        let Some(variable) = common.and_then(|values| values.into_iter().next()) else {
            return;
        };
        values.retain(|value| value != &variable);
        let values = if values.len() == 1 {
            format!("{},", values[0])
        } else {
            values.join(", ")
        };
        self.emit("R1714", "consider-using-in", &format!("Consider merging these comparisons with 'in' by using '{variable} {}in ({values})'. Use a set instead if elements are hashable.", if boolean.op == BoolOp::And { "not " } else { "" }), boolean.range());
    }
}
