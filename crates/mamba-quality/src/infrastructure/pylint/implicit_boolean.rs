//! Empty-value recommendations retain original inference and boolean-context limits.
use super::{display, operand_inference::Operand, resolution::Context, Checker};
use mamba_language::application::quality_syntax::python_ast::{CmpOp, Expr, UnaryOp};
use ruff_text_size::Ranged;

fn empty(value: &Expr) -> Option<(&'static str, &'static str)> {
    match value {
        Expr::List(value) if value.elts.is_empty() => Some(("list", "[]")),
        Expr::Tuple(value) if value.elts.is_empty() => Some(("tuple", "()")),
        Expr::Set(value) if value.elts.is_empty() => Some(("iterable", "iterable")),
        Expr::Dict(value) if value.items.is_empty() => Some(("dict", "{}")),
        _ => None,
    }
}
fn len(value: &Expr) -> bool {
    matches!(value, Expr::Call(call) if matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "len"))
}
impl Checker<'_> {
    fn implicit_types(&self, operand: &Operand) -> Option<(Vec<String>, bool)> {
        match operand {
            Operand::Literal(name) => Some((
                vec![(*name).into()],
                matches!(*name, "int" | "float" | "complex" | "bool"),
            )),
            Operand::Instance(class, origin) => Some((
                self.lineage(class.clone(), origin.clone())
                    .into_iter()
                    .map(|(class, _)| class.name.clone())
                    .collect(),
                self.class_member(class.clone(), origin.clone(), "__bool__", false)
                    .is_some(),
            )),
            Operand::Class(class, origin) => Some((
                Vec::new(),
                self.class_member(class.clone(), origin.clone(), "__bool__", false)
                    .is_some(),
            )),
            Operand::Function(_) | Operand::Generator => Some((Vec::new(), false)),
            Operand::Unknown => None,
        }
    }
    fn emit_implicit_len(&mut self, value: &Expr) {
        self.emit(
            "C1802",
            "use-implicit-booleaness-not-len",
            "Do not use `len(SEQUENCE)` without comparison to determine if a sequence is empty",
            value.range(),
        );
    }
    pub(super) fn check_implicit_boolean(&mut self, value: &Expr) {
        if let Expr::UnaryOp(unary) = value {
            if unary.op == UnaryOp::Not && len(&unary.operand) {
                self.emit_implicit_len(value);
            }
        }
        if let Expr::Call(call) = value {
            if !len(value)
                || !self
                    .implicit_conditions
                    .calls
                    .contains(&call.start().to_u32())
            {
                return;
            }
            let Some(argument) = call.arguments.args.first() else {
                return;
            };
            if matches!(
                argument,
                Expr::ListComp(_) | Expr::SetComp(_) | Expr::DictComp(_)
            ) {
                self.emit_implicit_len(value);
                return;
            }
            if self
                .slot_nodes(argument, Context::Scope(self.scopes.current_index()))
                .first()
                .is_some_and(|(value, _)| {
                    matches!(
                        value,
                        Expr::ListComp(_) | Expr::SetComp(_) | Expr::DictComp(_)
                    )
                })
            {
                // Inferred comprehension nodes are not original Instance nodes.
                return;
            }
            let Some((types, boolean)) = self.implicit_types(&self.operand(argument)) else {
                return;
            };
            if types.iter().any(|name| name == "range")
                || (!boolean
                    && types
                        .iter()
                        .any(|name| matches!(name.as_str(), "str" | "tuple" | "list" | "set")))
            {
                self.emit_implicit_len(value);
            }
        }
        let Expr::Compare(comparison) = value else {
            return;
        };
        if comparison.ops.len() != 1 {
            return;
        }
        let left = &comparison.operands[0];
        let right = &comparison.operands[1];
        let (target, (description, literal)) = match (empty(left), empty(right)) {
            (Some(empty), None) => (right, empty),
            (None, Some(empty)) => (left, empty),
            _ => return,
        };
        let operator = comparison.ops[0];
        if !matches!(
            operator,
            CmpOp::Eq | CmpOp::NotEq | CmpOp::Lt | CmpOp::LtE | CmpOp::Gt | CmpOp::GtE
        ) {
            return;
        }
        let view = self
            .unpacked_value(target, Context::Scope(self.scopes.current_index()))
            .is_some_and(|value| value.view.is_some());
        let Some((types, boolean)) = (if view {
            Some((Vec::new(), false))
        } else {
            self.implicit_types(&self.operand(target))
        }) else {
            return;
        };
        if boolean
            && !types
                .iter()
                .any(|name| matches!(name.as_str(), "tuple" | "list" | "dict" | "set"))
        {
            return;
        }
        let name = match target {
            Expr::Name(_) | Expr::Attribute(_) => display::expression(target, &self.source.text),
            Expr::Call(call) => format!(
                "{}(...)",
                display::expression(&call.func, &self.source.text)
            ),
            _ => "x".into(),
        };
        let suggestion = if operator == CmpOp::NotEq {
            if self
                .implicit_conditions
                .comparisons
                .contains(&comparison.start().to_u32())
            {
                name.clone()
            } else {
                format!("bool({name})")
            }
        } else {
            format!("not {name}")
        };
        self.emit("C1803", "use-implicit-booleaness-not-comparison", &format!("\"{name} {operator} {literal}\" can be simplified to \"{suggestion}\", if it is strictly a sequence, as an empty {description} is falsey"), comparison.range());
    }
}
