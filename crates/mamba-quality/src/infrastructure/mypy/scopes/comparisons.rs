//! Comparison inference retains each operand pair and its original result.
use super::*;
use mamba_language::application::quality_syntax::python_ast::ExprCompare;

impl Scopes {
    pub(in crate::infrastructure::mypy) fn comparison_details(
        &self,
        compare: &ExprCompare,
        environment: &BTreeMap<String, Type>,
    ) -> Option<Vec<types::comparisons::Comparison>> {
        let mut pairs = Vec::new();
        for (operands, &operator) in compare.operands.windows(2).zip(&compare.ops) {
            let left = self
                .context
                .infer_with_calls(&operands[0], environment, &|expression| {
                    self.expression_type(expression, environment)
                })?;
            let right =
                self.context
                    .infer_with_calls(&operands[1], environment, &|expression| {
                        self.expression_type(expression, environment)
                    })?;
            pairs.push(
                self.context
                    .dataclass_comparison(left, right, operator)
                    .or_else(|| self.context.sequence_comparison(left, right, operator))
                    .or_else(|| {
                        (self.system_version_operand(&operands[0], environment)
                            || self.system_version_operand(&operands[1], environment))
                        .then(|| self.context.version_comparison(left, right, operator))
                        .flatten()
                    })
                    .or_else(|| {
                        self.context.ordering_comparison(
                            left,
                            right,
                            operator,
                            self.context.module_target(&self.namespace),
                        )
                    })
                    .or_else(|| {
                        self.context.equality_comparison(
                            left,
                            right,
                            operator,
                            self.context.module_target(&self.namespace),
                        )
                    })
                    .or_else(|| {
                        types::comparisons::primitive_equality::comparison(left, right, operator)
                    })
                    .or_else(|| {
                        types::comparisons::none_identity::comparison(left, right, operator)
                    })?,
            );
        }
        Some(pairs)
    }
    pub(in crate::infrastructure::mypy) fn system_version_operand(
        &self,
        expression: &Expr,
        environment: &BTreeMap<String, Type>,
    ) -> bool {
        if let Expr::Subscript(subscript) = expression {
            return self.system_version_operand(&subscript.value, environment);
        }
        self.context
            .infer_with_calls(expression, environment, &|value| {
                self.expression_type(value, environment)
            })
            .is_some_and(|ty| self.context.system_version_items(ty).is_some())
    }
}
