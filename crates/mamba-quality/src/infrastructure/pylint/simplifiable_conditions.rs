//! Boolean suggestions reduce only name-free inferred terms.
use super::{display, Checker};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, Visitor},
    BoolOp, Expr, ExprBoolOp, ExprBooleanLiteral,
};
use ruff_text_size::Ranged;

#[derive(Default)]
struct Names(bool);
impl<'a> Visitor<'a> for Names {
    fn visit_expr(&mut self, value: &'a Expr) {
        if matches!(value, Expr::Name(name) if name.ctx.is_load()) {
            self.0 = true;
        } else if !self.0 {
            walk_expr(self, value);
        }
    }
}
pub(super) fn has_name(value: &Expr) -> bool {
    let mut names = Names::default();
    names.visit_expr(value);
    names.0
}
impl Checker<'_> {
    fn simplify_boolean(&self, value: &ExprBoolOp, reduced: &mut bool) -> Expr {
        let mut result = Vec::new();
        for child in &value.values {
            let child = match child {
                Expr::BoolOp(child) => self.simplify_boolean(child, reduced),
                child => child.clone(),
            };
            let truth = if has_name(&child) {
                None
            } else {
                self.simplification_truth(&child)
            };
            match truth {
                Some(truth) if (value.op == BoolOp::Or) == truth => {
                    result = vec![child];
                    break;
                }
                Some(_) => {}
                None => result.push(child),
            }
        }
        if result.is_empty() {
            result.push(Expr::BooleanLiteral(ExprBooleanLiteral {
                range: value.range,
                value: value.op == BoolOp::And,
                ..ExprBooleanLiteral::default()
            }));
        }
        *reduced |= result.len() < value.values.len();
        if result.len() == 1 {
            result.pop().expect("one simplified term")
        } else {
            let mut simplified = value.clone();
            simplified.values = result;
            Expr::BoolOp(simplified)
        }
    }
    pub(super) fn check_simplifiable_condition(&mut self, value: &Expr) {
        let Expr::BoolOp(boolean) = value else {
            return;
        };
        if !self
            .simplifiable_conditions
            .contains(&(value.start().to_u32(), value.end().to_u32()))
        {
            return;
        }
        let mut reduced = false;
        let simplified = self.simplify_boolean(boolean, &mut reduced);
        if !reduced {
            return;
        }
        let original = display::expression(value, &self.source.text);
        let suggestion = display::expression(&simplified, &self.source.text);
        let (rule, symbol, message) = if has_name(&simplified) {
            (
                "R1726",
                "simplifiable-condition",
                format!("Boolean condition \"{original}\" may be simplified to \"{suggestion}\""),
            )
        } else {
            (
                "R1727",
                "condition-evals-to-constant",
                format!("Boolean condition '{original}' will always evaluate to '{suggestion}'"),
            )
        };
        self.emit(rule, symbol, &message, value.range());
    }
}
