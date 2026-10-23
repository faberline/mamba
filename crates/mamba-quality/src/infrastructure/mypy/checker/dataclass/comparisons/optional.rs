//! A proved optional integer tail reports its rejected None arm and full union.
use super::*;
use quality_syntax::python_ast::CmpOp;

struct OptionalIntegerFailure {
    symbol: &'static str,
    whole_right: types::Type,
}

impl Checker<'_> {
    pub(super) fn visit_optional_integer_comparison(&mut self, compare: &ExprCompare) -> bool {
        let Some(failure) = self.optional_integer_comparison_failure(compare) else {
            return false;
        };
        // This path owns the complete chain only after every pair is proved.
        // Keep the ordinary fallback's operand visits once, in source order.
        for operand in &compare.operands {
            self.visit_expr(operand);
        }
        let range =
            ruff_text_size::TextRange::at(compare.start(), ruff_text_size::TextSize::new(1));
        self.emit_with_hint(
            "operator",
            format!(
                "Unsupported operand types for {} (\"{}\" and \"{}\")",
                failure.symbol,
                self.context.name(types::Type::Int),
                self.context.name(types::Type::None),
            ),
            &range,
            Some(format!(
                "Right operand is of type \"{}\"",
                self.context.name(failure.whole_right),
            )),
        );
        true
    }

    fn optional_integer_comparison_failure(
        &self,
        compare: &ExprCompare,
    ) -> Option<OptionalIntegerFailure> {
        let operands: &[Expr] = compare.operands.as_ref();
        let operators: &[CmpOp] = compare.ops.as_ref();
        if operators.is_empty() || operands.len() != operators.len() + 1 {
            return None;
        }
        for operand in &operands[..operators.len()] {
            if self.infer(operand) != Some(types::Type::Int) {
                return None;
            }
        }
        let whole_right = self.infer(operands.last()?)?;
        let node = self.context.node(whole_right)?;
        if !matches!(node.as_ref(), types::Node::Union(items)
            if items.len() == 2 && items.contains(&types::Type::Int)
                && items.contains(&types::Type::None))
        {
            return None;
        }
        for &operator in operators {
            let normal = self.context.ordering_comparison(
                types::Type::Int,
                types::Type::Int,
                operator,
                self.python_target,
            )?;
            if normal.failure.is_some() {
                return None;
            }
        }
        let (direct, reflected, symbol) = types::comparisons::method(*operators.last()?)?;
        let signature =
            self.context
                .class_method_for(types::Type::Int, direct, self.python_target)?;
        if !self.context.ordering_call(&signature, types::Type::None)?.1 {
            return None;
        }
        // Mypy's None member lookup uses the real object fallback. It must not
        // supply a reflected ordering operator for the rejected arm.
        let object = self
            .context
            .runtime_builtin_class_instance("object", self.python_target)?;
        if self
            .context
            .class_method_for(object, reflected, self.python_target)
            .is_some()
        {
            return None;
        }
        Some(OptionalIntegerFailure {
            symbol,
            whole_right,
        })
    }
}
