//! Stable Black retains source parentheses on nested assignment and delete targets.
use crate::expression::parentheses::parenthesized;
use crate::prelude::*;
use ruff_python_ast::token::parentheses_iterator;
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Expr, ExprContext};
use ruff_text_size::Ranged;

pub(crate) fn target_forces_break(expression: &Expr, context: &PyFormatContext) -> bool {
    struct Find<'a, 'ast> {
        context: &'a PyFormatContext<'ast>,
        found: bool,
    }
    impl<'expr> Visitor<'expr> for Find<'_, '_> {
        fn visit_expr(&mut self, expression: &'expr Expr) {
            if self.found {
                return;
            }
            let range = parentheses_iterator(expression.into(), None, self.context.tokens())
                .last()
                .unwrap_or(expression.range());
            let comments = self
                .context
                .comments()
                .leading_dangling_trailing(expression);
            self.found = comments
                .leading
                .iter()
                .chain(comments.dangling)
                .chain(comments.trailing)
                .any(|comment| {
                    comment.line_position().is_own_line() && range.contains_range(comment.range())
                });
            if !self.found {
                visitor::walk_expr(self, expression);
            }
        }
    }
    let mut find = Find {
        context,
        found: false,
    };
    find.visit_expr(expression);
    find.found
}

pub(super) fn target_layers(expression: &Expr, context: &PyFormatContext) -> usize {
    let ctx = match expression {
        Expr::Name(node) => node.ctx,
        Expr::Attribute(node) => node.ctx,
        Expr::Subscript(node) => node.ctx,
        Expr::Starred(node) => node.ctx,
        Expr::List(node) => node.ctx,
        Expr::Tuple(node) => node.ctx,
        _ => return 1,
    };
    if ctx == ExprContext::Load {
        return 1;
    }
    parentheses_iterator(expression.into(), None, context.tokens())
        .count()
        .max(1)
}

pub(super) fn format_layers<'ast>(
    content: &dyn Format<PyFormatContext<'ast>>,
    count: usize,
    f: &mut PyFormatter<'ast, '_>,
) -> FormatResult<()> {
    if count == 0 {
        return content.fmt(f);
    }
    parenthesized(
        "(",
        &format_with(|f| format_layers(content, count - 1, f)),
        ")",
    )
    .fmt(f)
}
