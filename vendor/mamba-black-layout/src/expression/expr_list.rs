use ruff_formatter::prelude::format_with;
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::{Expr, ExprList};
use ruff_text_size::Ranged;

use crate::expression::parentheses::{
    NeedsParentheses, OptionalParentheses, empty_parenthesized, parenthesized,
};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprList;

impl FormatNodeRule<ExprList> for FormatExprList {
    fn fmt_fields(&self, item: &ExprList, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprList {
            range: _,
            node_index: _,
            elts,
            ctx: _,
        } = item;

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        if elts.is_empty() {
            return empty_parenthesized("[", dangling, "]").fmt(f);
        }

        let items = format_with(|f: &mut PyFormatter| {
            if f.options().style_policy().is_some()
                && elts.len() > 1
                && elts
                    .iter()
                    .any(|item| crate::black_multiline::contains(item, f.context().source()))
            {
                expand_parent().fmt(f)?;
            }
            if let [single] = elts.as_slice()
                && f.options().style_policy().is_some()
            {
                let content = format_with(|f| format_singleton(single, f));
                f.join_comma_separated(item.end())
                    .entry(single, &content)
                    .finish()
            } else {
                let native = f.options().style_policy().is_some();
                f.join_comma_separated(item.end())
                    .entries(elts.iter().map(|element| {
                        (
                            element,
                            format_with(move |f: &mut PyFormatter| {
                                if native
                                    && element.is_if_expr()
                                    && !f.context().is_expression_parenthesized(element.into())
                                {
                                    format_singleton(element, f)
                                } else {
                                    element.format().fmt(f)
                                }
                            }),
                        )
                    }))
                    .finish()
            }
        });

        parenthesized("[", &items, "]")
            .with_dangling_comments(dangling)
            .fmt(f)
    }
}

/// Black makes a lone item's redundant parentheses invisible. A multiline
/// expression can still need that group; tuple and generator delimiters stay.
pub(super) fn format_singleton(item: &Expr, f: &mut PyFormatter) -> FormatResult<()> {
    use crate::builders::parenthesize_if_expands;
    use crate::expression::parentheses::Parentheses;
    let comments = f.context().comments();
    let node_comments = comments.leading_dangling_trailing(item);
    if !node_comments.has_leading()
        && !node_comments.has_trailing_own_line()
        && crate::expression::has_own_parentheses(item, f.context()).is_some()
    {
        item.format().with_options(Parentheses::Never).fmt(f)
    } else if comments.has_leading(item) || comments.has_trailing(item) {
        item.format().fmt(f)
    } else if item.is_if_expr()
        || f.context().is_expression_parenthesized(item.into())
            && matches!(
                item,
                Expr::If(_) | Expr::BinOp(_) | Expr::BoolOp(_) | Expr::Compare(_)
            )
    {
        parenthesize_if_expands(&item.format().with_options(Parentheses::Never)).fmt(f)
    } else {
        item.format().with_options(Parentheses::Never).fmt(f)
    }
}

impl NeedsParentheses for ExprList {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}
