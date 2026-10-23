//! Unittest constant assertions use literal arguments rather than inferred aliases.
use super::{callables::Target, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, ExprCall};
use ruff_text_size::Ranged;
use std::collections::BTreeSet;
fn constant(value: &Expr) -> bool {
    matches!(
        value,
        Expr::StringLiteral(_)
            | Expr::NumberLiteral(_)
            | Expr::BooleanLiteral(_)
            | Expr::NoneLiteral(_)
            | Expr::BytesLiteral(_)
    )
}
impl Checker<'_> {
    pub(super) fn check_unittest_call(&mut self, call: &ExprCall) {
        let Expr::Attribute(value) = call.func.as_ref() else {
            return;
        };
        let name = value.attr.as_str();
        let count = match name {
            "assertTrue" | "assertFalse" => 1,
            "assertEqual" | "assertNotEqual" => 2,
            _ => return,
        };
        let Some(resolved) = self.expression_target(&value.value) else {
            return;
        };
        let Target::Instance(class, origin) = resolved.target else {
            return;
        };
        let unittest = self.lineage(class, origin).iter().any(|(class, origin)| {
            class.bases.iter().any(|base| {
                matches!(
                    self.stdlib_name_in(base, origin.clone(), &mut BTreeSet::new())
                        .as_deref(),
                    Some("unittest.TestCase" | "unittest.case.TestCase")
                )
            })
        });
        if !unittest
            || call.arguments.args.len() < count
            || !call.arguments.args[..count].iter().all(constant)
        {
            return;
        }
        let values: Vec<_> = call.arguments.args[..count]
            .iter()
            .map(|value| super::display::expression(value, ""))
            .collect();
        let value = if count == 1 {
            values[0].clone()
        } else {
            format!("({})", values.join(", "))
        };
        self.emit(
            "W1503",
            "redundant-unittest-assert",
            &format!("Redundant use of {name} with constant value {value}"),
            call.range(),
        );
    }
}
