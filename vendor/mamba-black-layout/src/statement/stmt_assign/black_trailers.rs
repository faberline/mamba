//! Stable Black's bracket omission after a leading magic comma.
use crate::context::with_forced_bracket;
use crate::expression::parentheses::{Parentheses, parenthesized};
use crate::prelude::*;
use ruff_formatter::FormatOptions;
use ruff_python_ast::{
    Expr, Operator, StmtAssign,
    token::{TokenKind, parentheses_iterator},
    visitor::{self, Visitor},
};
use ruff_text_size::{Ranged, TextRange};

fn repeated_operator(expr: &Expr, op: Operator, context: &PyFormatContext) -> usize {
    match expr {
        Expr::BinOp(binary) if binary.op == op => {
            1 + [&*binary.left, &*binary.right]
                .iter()
                .map(|child| {
                    if context.is_expression_parenthesized((*child).into()) {
                        0
                    } else {
                        repeated_operator(child, op, context)
                    }
                })
                .sum::<usize>()
        }
        _ => 0,
    }
}

fn useful_end(expr: &Expr, context: &PyFormatContext) -> bool {
    if context.is_expression_parenthesized(expr.into()) {
        return true;
    }
    match expr {
        Expr::BinOp(binary) => useful_end(&binary.right, context),
        Expr::BoolOp(boolean) => boolean
            .values
            .last()
            .is_some_and(|last| useful_end(last, context)),
        Expr::UnaryOp(unary) => useful_end(&unary.operand, context),
        Expr::Call(call) => !call.arguments.is_empty(),
        Expr::Tuple(tuple) => !tuple.elts.is_empty(),
        Expr::List(list) => !list.elts.is_empty(),
        Expr::Set(set) => !set.elts.is_empty(),
        Expr::Dict(dict) => !dict.items.is_empty(),
        _ => false,
    }
}

fn can_omit(expr: &Expr, context: &PyFormatContext) -> bool {
    let (first, last) = match expr {
        Expr::BoolOp(boolean) if boolean.values.len() == 2 => {
            (&boolean.values[0], &boolean.values[1])
        }
        Expr::BoolOp(_) | Expr::If(_) => return false,
        Expr::BinOp(binary) if repeated_operator(expr, binary.op, context) <= 1 => {
            (&*binary.left, &*binary.right)
        }
        Expr::BinOp(_) => return false,
        _ => return true,
    };
    context.is_expression_parenthesized(first.into()) || useful_end(last, context)
}

struct Brackets<'a, 'ast> {
    context: &'a PyFormatContext<'ast>,
    ranges: Vec<TextRange>,
}
impl<'expr> Visitor<'expr> for Brackets<'_, '_> {
    fn visit_expr(&mut self, expr: &'expr Expr) {
        if self.context.is_expression_parenthesized(expr.into()) {
            if let Some(range) =
                parentheses_iterator(expr.into(), None, self.context.tokens()).last()
            {
                self.ranges.push(range);
                return;
            }
        }
        match expr {
            Expr::Call(call) => {
                self.visit_expr(&call.func);
                if !call.arguments.is_empty() {
                    self.ranges.push(call.arguments.range());
                }
            }
            Expr::Subscript(subscript) => {
                self.visit_expr(&subscript.value);
                if let Some(open) = self
                    .context
                    .tokens()
                    .in_range(TextRange::new(subscript.value.end(), subscript.end()))
                    .iter()
                    .find(|token| token.kind() == TokenKind::Lsqb)
                {
                    self.ranges
                        .push(TextRange::new(open.start(), subscript.end()));
                }
            }
            Expr::Tuple(_) | Expr::List(_) | Expr::Set(_) | Expr::Dict(_) => {
                self.ranges.push(expr.range())
            }
            _ => visitor::walk_expr(self, expr),
        }
    }
}

pub(super) fn format(item: &StmtAssign, f: &mut PyFormatter) -> FormatResult<bool> {
    if f.options().style_policy().is_none()
        || f.context().forced_bracket().is_some()
        || f.context().comments().contains_comments(item.into())
        || !item.targets.iter().any(|target| {
            if target.is_subscript_expr()
                && crate::expression::black_trailers::has_magic(target, f.context())
            {
                return true;
            }
            let last = match target {
                Expr::Tuple(tuple) if tuple.elts.len() > 1 => tuple.elts.last(),
                Expr::List(list) => list.elts.last(),
                _ => None,
            };
            last.is_some_and(|last| {
                crate::other::commas::has_magic_trailing_comma(
                    TextRange::new(last.end(), target.end()),
                    f.context(),
                )
            })
        })
    {
        return Ok(false);
    }
    if !can_omit(&item.value, f.context()) {
        for (index, target) in item.targets.iter().enumerate() {
            super::FormatTargetWithEqualOperator {
                target,
                preserve_parentheses: index == 0,
            }
            .fmt(f)?;
        }
        parenthesized(
            "(",
            &item.value.format().with_options(Parentheses::Never),
            ")",
        )
        .with_force_expansion(true)
        .fmt(f)?;
        return Ok(true);
    }
    let mut brackets = Brackets {
        context: f.context(),
        ranges: Vec::new(),
    };
    brackets.visit_expr(&item.value);
    if brackets.ranges.len() < 2 {
        return Ok(false);
    }
    brackets.ranges.sort_by_key(|range| range.end());
    let last = *brackets.ranges.last().unwrap();
    if f.context()
        .tokens()
        .in_range(last)
        .iter()
        .rev()
        .filter(|token| !token.kind().is_trivia())
        .nth(1)
        .is_some_and(|token| token.kind() == TokenKind::Comma)
    {
        return Ok(false);
    }
    let selected = brackets.ranges[brackets.ranges.len() - 2];
    let suffix = &f.context().source()[TextRange::new(selected.end(), item.value.end())];
    if ruff_formatter::black_width::measure(suffix) > usize::from(f.options().line_width().value())
    {
        return Ok(false);
    }
    with_forced_bracket(selected, f, |f| super::FormatStmtAssign.fmt_fields(item, f))?;
    Ok(true)
}
