//! Direct and reflected comparison failures keep their distinct source spans.
use super::super::*;
use quality_syntax::python_ast::ExprCompare;
use types::comparisons::Failure;
mod optional;

impl Checker<'_> {
    pub(in crate::infrastructure::mypy::checker) fn visit_dataclass_comparison(
        &mut self,
        compare: &ExprCompare,
    ) -> bool {
        let Some(pairs) = self.scopes.comparison_details(compare, &self.environment()) else {
            return self.visit_optional_integer_comparison(compare);
        };
        self.visit_expr(&compare.operands[0]);
        for (index, pair) in pairs.into_iter().enumerate() {
            self.check_sequence_comparison(compare, &pair, &compare.operands[index..index + 2]);
            if matches!(pair.failure, Some(Failure::Right)) {
                if let Expr::Call(call) = &compare.operands[index + 1] {
                    if let Some((name, _, _)) = types::comparisons::method(compare.ops[index]) {
                        if let Some((expected, _)) =
                            self.context.comparison_parameter(pair.left, name)
                        {
                            if self
                                .scopes
                                .callable_expression_in(&call.func, &self.environment())
                                .is_some_and(|signature| !signature.generic.is_empty())
                                && self
                                    .scopes
                                    .call_return_context(call, &self.environment(), Some(expected))
                                    .is_some_and(|ty| self.context.accepts(expected, ty))
                            {
                                self.expected_calls
                                    .insert((call.start(), call.end()), expected);
                                self.visit_expr(&compare.operands[index + 1]);
                                continue;
                            }
                        }
                    }
                }
            }
            self.visit_expr(&compare.operands[index + 1]);
            let (message, range) = match pair.failure {
                Some(Failure::Left) => (
                    format!(
                        "Unsupported left operand type for {} (\"{}\")",
                        pair.symbol,
                        self.context.name(pair.left)
                    ),
                    compare.range(),
                ),
                Some(Failure::Right) => (
                    format!(
                        "Unsupported operand types for {} (\"{}\" and \"{}\")",
                        pair.symbol,
                        self.context.name(pair.left),
                        self.context.name(pair.right)
                    ),
                    compare.operands[index + 1].range(),
                ),
                Some(Failure::Reflected) => {
                    let symbol = match pair.symbol {
                        "<" => ">",
                        ">" => "<",
                        "<=" => ">=",
                        _ => "<=",
                    };
                    (
                        format!(
                            "Unsupported operand types for {symbol} (\"{}\" and \"{}\")",
                            self.context.name(pair.right),
                            self.context.name(pair.left)
                        ),
                        ruff_text_size::TextRange::at(
                            compare.operands[index].start(),
                            ruff_text_size::TextSize::new(1),
                        ),
                    )
                }
                None => continue,
            };
            self.emit("operator", message, &range);
        }
        true
    }
}
