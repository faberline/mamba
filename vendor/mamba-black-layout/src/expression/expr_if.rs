use ruff_formatter::{FormatContext, FormatRuleWithOptions, write};
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::{Expr, ExprIf};
use ruff_text_size::Ranged;

use crate::comments::{SourceComment, leading_comments};
use crate::expression::parentheses::{
    NeedsParentheses, OptionalParentheses, in_parentheses_only_group,
    in_parentheses_only_soft_line_break_or_space,
};
use crate::prelude::*;

#[derive(Default, Copy, Clone)]
pub enum ExprIfLayout {
    #[default]
    Default,

    /// The [`ExprIf`] is nested inside another [`ExprIf`], so it should not be given a new
    /// group. For example, avoid grouping the `else` clause in:
    /// ```python
    /// clone._iterable_class = (
    ///     NamedValuesListIterable
    ///     if named
    ///     else FlatValuesListIterable
    ///     if flat
    ///     else ValuesListIterable
    /// )
    /// ```
    Nested,
}

#[derive(Default)]
pub struct FormatExprIf {
    layout: ExprIfLayout,
}

impl FormatRuleWithOptions<ExprIf, PyFormatContext<'_>> for FormatExprIf {
    type Options = ExprIfLayout;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.layout = options;
        self
    }
}

impl FormatNodeRule<ExprIf> for FormatExprIf {
    fn fmt_fields(&self, item: &ExprIf, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprIf {
            range: _,
            node_index: _,
            test,
            body,
            orelse,
        } = item;
        let comments = f.context().comments().clone();

        let inner = format_with(|f: &mut PyFormatter| {
            if f.options().style_policy().is_some()
                && [&**body, &**test, &**orelse]
                    .iter()
                    .any(|value| crate::black_multiline::contains(value, f.context().source()))
            {
                expand_parent().fmt(f)?;
            }
            // We place `if test` and `else orelse` on a single line, so the `test` and `orelse` leading
            // comments go on the line before the `if` or `else` instead of directly ahead `test` or
            // `orelse`
            write!(
                f,
                [
                    body.format(),
                    in_parentheses_only_soft_line_break_or_space(),
                    leading_comments(before_keyword(
                        test,
                        comments.leading(test.as_ref()),
                        f.context()
                    )),
                    token("if"),
                    space(),
                    test.format(),
                    in_parentheses_only_soft_line_break_or_space(),
                    leading_comments(before_keyword(
                        orelse,
                        comments.leading(orelse.as_ref()),
                        f.context()
                    )),
                    token("else"),
                    space(),
                ]
            )?;

            FormatOrElse { orelse }.fmt(f)
        });

        match self.layout {
            ExprIfLayout::Default => in_parentheses_only_group(&inner).fmt(f),
            ExprIfLayout::Nested => inner.fmt(f),
        }
    }
}

fn before_keyword<'c>(
    expression: &Expr,
    comments: &'c [SourceComment],
    context: &PyFormatContext,
) -> &'c [SourceComment] {
    if context.options().style_policy().is_some()
        && context.is_expression_parenthesized(expression.into())
        && let Some(range) =
            ruff_python_ast::token::parentheses_iterator(expression.into(), None, context.tokens())
                .last()
    {
        return &comments[..comments.partition_point(|comment| comment.start() < range.start())];
    }
    comments
}

impl NeedsParentheses for ExprIf {
    fn needs_parentheses(
        &self,
        parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        if parent.is_expr_await() {
            OptionalParentheses::Always
        } else {
            OptionalParentheses::Multiline
        }
    }
}

#[derive(Debug)]
struct FormatOrElse<'a> {
    orelse: &'a Expr,
}

impl Format<PyFormatContext<'_>> for FormatOrElse<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        match self.orelse {
            Expr::If(expr) if !f.context().is_expression_parenthesized(expr.into()) => {
                if f.options().style_policy().is_some() {
                    expr.format().fmt(f)
                } else {
                    write!(f, [expr.format().with_options(ExprIfLayout::Nested)])
                }
            }
            _ => write!(f, [in_parentheses_only_group(&self.orelse.format())]),
        }
    }
}
