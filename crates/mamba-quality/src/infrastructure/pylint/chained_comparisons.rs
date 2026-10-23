//! Mixed comparison groups retain the original sorted operator description.
use super::Checker;
use mamba_language::application::quality_syntax::python_ast::{CmpOp, ExprCompare};
use ruff_text_size::Ranged;
use std::collections::BTreeSet;

impl Checker<'_> {
    pub(super) fn check_chained_comparison(&mut self, comparison: &ExprCompare) {
        let group = |operator: CmpOp| match operator {
            CmpOp::Is | CmpOp::IsNot => 1,
            CmpOp::In | CmpOp::NotIn => 2,
            _ => 0,
        };
        let Some(first) = comparison.ops.first() else {
            return;
        };
        if comparison
            .ops
            .iter()
            .all(|operator| group(*operator) == group(*first))
        {
            return;
        }
        let operators: BTreeSet<_> = comparison.ops.iter().map(ToString::to_string).collect();
        let mut operators: Vec<_> = operators
            .iter()
            .map(|operator| format!("'{operator}'"))
            .collect();
        let last = operators.pop().unwrap();
        let description = format!("{} and {last}", operators.join(", "));
        self.emit(
            "W3601",
            "bad-chained-comparison",
            &format!(
                "Suspicious {}-part chained comparison using semantically incompatible operators ({description})",
                comparison.ops.len()
            ),
            comparison.range(),
        );
    }
}
