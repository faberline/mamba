//! Strict sequence comparison errors retain the original operator span.
use super::*;
use quality_syntax::python_ast::ExprCompare;

impl Checker<'_> {
    pub(super) fn check_sequence_comparison(
        &mut self,
        compare: &ExprCompare,
        pair: &types::comparisons::Comparison,
        operands: &[Expr],
    ) {
        if self.suppress_comparison_overlap
            || pair.left == types::Type::None
            || pair.right == types::Type::None
        {
            return;
        }
        let identity = matches!(pair.symbol, "is" | "is not");
        if !identity
            && (self.context.custom_equality(pair.left) || self.context.custom_equality(pair.right))
        {
            return;
        }
        let (kind, left, right) = if matches!(pair.symbol, "in" | "not in")
            && self.context.embedded_sequence(pair.right)
        {
            let Some(item) = self.context.iterable_item(pair.right) else {
                return;
            };
            if self.context.type_overlap(pair.left, item) {
                return;
            }
            ("container", pair.left, item)
        } else if matches!(pair.symbol, "==" | "!=" | "is" | "is not")
            && (self.context.embedded_sequence(pair.left)
                || self.context.embedded_sequence(pair.right))
        {
            if self.context.type_overlap(pair.left, pair.right) {
                return;
            }
            let left = self
                .context
                .literal_expression(&operands[0])
                .unwrap_or(pair.left);
            let right = self
                .context
                .literal_expression(&operands[1])
                .unwrap_or(pair.right);
            (if identity { "identity" } else { "equality" }, left, right)
        } else {
            return;
        };
        if self.loop_overlap(
            kind,
            left,
            right,
            compare.range(),
            self.context.source_declaration_site(compare),
        ) {
            return;
        }
        let message = if kind == "container" {
            format!("Non-overlapping container check (element type: \"{}\", container item type: \"{}\")", self.context.name(left), self.context.name(right))
        } else {
            format!(
                "Non-overlapping {} check (left operand type: \"{}\", right operand type: \"{}\")",
                kind,
                self.context.name(left),
                self.context.name(right)
            )
        };
        self.emit("comparison-overlap", message, compare);
    }
}
