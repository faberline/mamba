//! Container recommendations retain raw constructor and concatenation shapes.
use super::{display, Checker};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    Expr, ExprCall, InterpolatedStringElement, Operator, Stmt,
};
use ruff_text_size::Ranged;
use std::collections::BTreeSet;

fn pair(expression: &Expr) -> Option<[&Expr; 2]> {
    let values = match expression {
        Expr::Tuple(value) => &value.elts,
        Expr::List(value) => &value.elts,
        _ => return None,
    };
    if let [left, right] = values.as_slice() {
        Some([left, right])
    } else {
        None
    }
}

fn concatenated_name(expression: &Expr) -> Option<&str> {
    match expression {
        Expr::Name(value) => Some(value.id.as_str()),
        Expr::FString(value) => {
            let mut interpolations = value.value.elements().filter_map(|element| {
                if let InterpolatedStringElement::Interpolation(value) = element {
                    Some(value)
                } else {
                    None
                }
            });
            let value = interpolations.next()?;
            if interpolations.next().is_some() {
                return None;
            }
            if let Expr::Name(name) = value.expression.as_ref() {
                Some(name.id.as_str())
            } else {
                None
            }
        }
        _ => None,
    }
}

#[derive(Default)]
struct Joins(BTreeSet<u32>);
impl<'a> Visitor<'a> for Joins {
    fn visit_body(&mut self, body: &'a [Stmt]) {
        for (index, statement) in body.iter().enumerate() {
            if let (Some(Stmt::Assign(initial)), Stmt::For(loop_)) =
                (index.checked_sub(1).map(|index| &body[index]), statement)
            {
                if let (Expr::StringLiteral(_), Expr::Name(target), [Stmt::AugAssign(assignment)]) = (
                    initial.value.as_ref(),
                    loop_.target.as_ref(),
                    loop_.body.as_slice(),
                ) {
                    if let Expr::Name(result) = assignment.target.as_ref() {
                        if assignment.op == Operator::Add && initial.targets.iter().any(|value| matches!(value, Expr::Name(name) if name.id.as_str() == result.id.as_str()))
                            && concatenated_name(&assignment.value) == Some(target.id.as_str()) {
                            self.0.insert(assignment.start().to_u32());
                        }
                    }
                }
            }
            self.visit_stmt(statement);
        }
    }
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        walk_stmt(self, statement);
    }
}
pub(super) fn joins(body: &[Stmt]) -> BTreeSet<u32> {
    let mut joins = Joins::default();
    joins.visit_body(body);
    joins.0
}

impl Checker<'_> {
    pub(super) fn check_dict_comprehension_constructor(&mut self, call: &ExprCall) {
        let (Expr::Name(name), Some(Expr::ListComp(comprehension))) =
            (call.func.as_ref(), call.arguments.args.first())
        else {
            return;
        };
        if name.id.as_str() != "dict"
            || matches!(comprehension.elt.as_ref(), Expr::Call(_) | Expr::Starred(_))
        {
            return;
        }
        if let Expr::If(conditional) = comprehension.elt.as_ref() {
            if let (Some([left_key, left_value]), Some([right_key, right_value])) =
                (pair(&conditional.body), pair(&conditional.orelse))
            {
                if display::expression(left_key, &self.source.text)
                    != display::expression(right_key, &self.source.text)
                    && display::expression(left_value, &self.source.text)
                        != display::expression(right_value, &self.source.text)
                {
                    return;
                }
            }
        }
        self.emit(
            "R1717",
            "consider-using-dict-comprehension",
            "Consider using a dictionary comprehension",
            call.range(),
        );
    }
    pub(super) fn check_join_assignment(
        &mut self,
        value: &mamba_language::application::quality_syntax::python_ast::StmtAugAssign,
    ) {
        if self.join_assignments.contains(&value.start().to_u32()) {
            self.emit(
                "R1713",
                "consider-using-join",
                "Consider using str.join(sequence) for concatenating strings from an iterable",
                value.range(),
            );
        }
    }
}
