//! Comparison diagnostics retain checker order and binary-only boundaries.
use super::{
    callables::Target,
    comparison_constants, display,
    values::{self, Value},
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::{CmpOp, Expr, ExprCompare, Number};
use ruff_text_size::Ranged;

impl Checker<'_> {
    fn check_comparison_identity(
        &mut self,
        left: &Expr,
        operator: CmpOp,
        right: &Expr,
        comparison: &ExprCompare,
    ) {
        let same = if comparison_constants::constant(left) && comparison_constants::constant(right)
        {
            comparison_constants::same(left, right)
        } else {
            match (left, right) {
                (Expr::Name(left), Expr::Name(right)) => left.id == right.id,
                (Expr::Attribute(_), Expr::Attribute(_)) => values::path(left)
                    .zip(values::path(right))
                    .is_some_and(|(left, right)| left == right),
                _ => false,
            }
        };
        if same {
            let left = comparison_constants::value_text(left, &self.source.text);
            let right = comparison_constants::value_text(right, &self.source.text);
            self.emit(
                "R0124",
                "comparison-with-itself",
                &format!("Redundant comparison - {left} {operator} {right}"),
                comparison.range(),
            );
        }
    }
    fn check_comparison_singleton(
        &mut self,
        left: &Expr,
        operator: CmpOp,
        right: &Expr,
        comparison: &ExprCompare,
    ) {
        let singleton = |value: &Expr| match value {
            Expr::BooleanLiteral(value) => Some(Some(value.value)),
            Expr::NoneLiteral(_) => Some(None),
            _ => None,
        };
        let Some((singleton, other)) = singleton(left)
            .map(|value| (value, right))
            .or_else(|| singleton(right).map(|value| (value, left)))
        else {
            return;
        };
        let original = display::expression(&Expr::Compare(comparison.clone()), &self.source.text);
        let left = display::expression(left, &self.source.text);
        let right = display::expression(right, &self.source.text);
        let absence = operator == CmpOp::NotEq;
        let mut suggestion = format!("'{left} is{}{right}'", if absence { " not " } else { " " });
        if let Some(singleton) = singleton {
            let truthiness = singleton != absence;
            let other = display::expression(other, &self.source.text);
            let truth = if truthiness
                && !self
                    .comparison_conditions
                    .contains(&comparison.start().to_u32())
            {
                format!("bool({other})")
            } else if truthiness {
                other
            } else {
                format!("not {other}")
            };
            suggestion.push_str(&format!(
                " if checking for the singleton value {}, or '{truth}' if testing for {}",
                if singleton { "True" } else { "False" },
                if truthiness {
                    "truthiness"
                } else {
                    "falsiness"
                }
            ));
        }
        self.emit(
            "C0121",
            "singleton-comparison",
            &format!("Comparison '{original}' should be {suggestion}"),
            comparison.range(),
        );
    }
    fn check_comparison_membership(&mut self, right: &Expr) {
        let dictionary_view = self
            .unpacked_value(
                right,
                super::resolution::Context::Scope(self.scopes.current_index()),
            )
            .is_some_and(|value| value.dictionary && value.view.is_some());
        let scalar = matches!(right, Expr::EllipsisLiteral(_))
            || matches!(right, Expr::UnaryOp(value) if matches!(self.slot_scalar(&value.operand,super::resolution::Context::Scope(self.scopes.current_index())),Value::Number(_) | Value::Boolean(_)))
            || self.comparison_nan_constant(
                right,
                super::resolution::Context::Scope(self.scopes.current_index()),
                &mut Default::default(),
            );
        let unsupported = !dictionary_view
            && (scalar
                || match self.expression_target(right) {
                    Some(resolved) => match resolved.target {
                        Target::Value(value) => matches!(
                            self.expand_value(
                                (*value).clone(),
                                &resolved.context,
                                &mut Default::default()
                            ),
                            Value::Number(_) | Value::Boolean(_) | Value::Scalar(_)
                        ),
                        Target::Instance(class, context) => {
                            ["__contains__", "__iter__", "__getitem__"]
                                .iter()
                                .all(|name| {
                                    self.class_member(class.clone(), context.clone(), name, true)
                                        .is_none()
                                })
                        }
                        _ => false,
                    },
                    _ => false,
                });
        if unsupported {
            self.emit(
                "E1135",
                "unsupported-membership-test",
                &format!(
                    "Value '{}' doesn't support membership test",
                    display::expression(right, &self.source.text)
                ),
                right.range(),
            );
        }
    }
    pub(super) fn check_comparison(&mut self, comparison: &ExprCompare) {
        let Some(&operator) = comparison.ops.first() else {
            return;
        };
        let Some(left) = comparison.operands.first() else {
            return;
        };
        let Some(right) = comparison.operands.get(1) else {
            return;
        };
        if matches!(
            operator,
            CmpOp::Eq | CmpOp::NotEq | CmpOp::Lt | CmpOp::Gt | CmpOp::LtE | CmpOp::GtE
        ) && self.comparison_callable(left) != self.comparison_callable(right)
        {
            self.emit(
                "W0143",
                "comparison-with-callable",
                "Comparing against a callable, did you omit the parenthesis?",
                comparison.range(),
            );
        }
        self.check_comparison_identity(left, operator, right, comparison);
        if matches!(
            operator,
            CmpOp::Is | CmpOp::IsNot | CmpOp::Eq | CmpOp::NotEq
        ) {
            self.check_comparison_type(left, right, comparison.range());
        }
        if comparison_constants::constant(left) && comparison_constants::constant(right) {
            self.emit(
                "R0133",
                "comparison-of-constants",
                &format!(
                    "Comparison between constants: \"{} {operator} {}\" has a constant value",
                    display::expression(left, &self.source.text),
                    display::expression(right, &self.source.text)
                ),
                comparison.range(),
            );
        }
        if comparison.ops.len() == 1 {
            if matches!(operator, CmpOp::Eq | CmpOp::NotEq) {
                self.check_comparison_singleton(left, operator, right, comparison);
            }
            if matches!(
                operator,
                CmpOp::Eq | CmpOp::NotEq | CmpOp::Is | CmpOp::IsNot
            ) {
                let nan_left = self.comparison_nan(left);
                if nan_left || self.comparison_nan(right) {
                    let original =
                        display::expression(&Expr::Compare(comparison.clone()), &self.source.text);
                    let other =
                        display::expression(if nan_left { right } else { left }, &self.source.text);
                    self.emit(
                        "W0177",
                        "nan-comparison",
                        &format!(
                            "Comparison '{original}' should be '{}math.isnan({other})'",
                            if matches!(operator, CmpOp::NotEq | CmpOp::IsNot) {
                                "not "
                            } else {
                                ""
                            }
                        ),
                        comparison.range(),
                    );
                }
            }
            if matches!(operator,CmpOp::Is | CmpOp::IsNot) && matches!(right, Expr::StringLiteral(_) | Expr::BytesLiteral(_) | Expr::List(_) | Expr::Dict(_) | Expr::Set(_) | Expr::NumberLiteral(mamba_language::application::quality_syntax::python_ast::ExprNumberLiteral { value: Number::Int(_) | Number::Float(_), .. })) {
                let left = display::expression(left, &self.source.text);
                let right = display::expression(right, &self.source.text);
                let (wrong, correct) = if operator == CmpOp::IsNot { ("is not", "!=") } else { ("is", "==") };
                self.emit("R0123", "literal-comparison", &format!("In '{left} {wrong} {right}', use '{correct}' when comparing constant literals not '{wrong}' ('{left} {correct} {right}')"), comparison.range());
            }
            if matches!(operator, CmpOp::In | CmpOp::NotIn) {
                self.check_comparison_membership(right);
            }
        }
    }
}
