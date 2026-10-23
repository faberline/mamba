//! Version-info equality checks use the typed runtime tuple domain.
use super::*;
use quality_syntax::python_ast::{CmpOp, ExprCompare};

impl Checker<'_> {
    pub(super) fn check_version_info_equality(&mut self, compare: &ExprCompare) {
        if compare.ops.len() != 1 || !matches!(compare.ops[0], CmpOp::Eq | CmpOp::NotEq) {
            return;
        }
        let [left, right] = compare.operands.as_ref() else {
            return;
        };
        let environment = self.environment();
        if !self.scopes.system_version_operand(left, &environment)
            && !self.scopes.system_version_operand(right, &environment)
        {
            return;
        }
        let Some((left_type, right_type)) = self.infer(left).zip(self.infer(right)) else {
            return;
        };
        if self.context.version_overlap(left_type, right_type) {
            return;
        }
        let left_type = self.context.literal_expression(left).unwrap_or(left_type);
        let right_type = self.context.literal_expression(right).unwrap_or(right_type);
        if !self.loop_overlap(
            "equality",
            left_type,
            right_type,
            compare.range(),
            self.context.source_declaration_site(compare),
        ) {
            self.emit("comparison-overlap", format!("Non-overlapping equality check (left operand type: \"{}\", right operand type: \"{}\")", self.context.name(left_type), self.context.name(right_type)), compare);
        }
    }
}
