use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprSet;
use ruff_text_size::Ranged;

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses, parenthesized};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprSet;

impl FormatNodeRule<ExprSet> for FormatExprSet {
    fn fmt_fields(&self, item: &ExprSet, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprSet {
            range: _,
            node_index: _,
            elts,
        } = item;
        // That would be a dict expression
        assert!(!elts.is_empty());
        // Avoid second mutable borrow of f
        let joined = format_with(|f: &mut PyFormatter| {
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
                let content = format_with(|f| super::expr_list::format_singleton(single, f));
                f.join_comma_separated(item.end())
                    .entry(single, &content)
                    .finish()
            } else {
                f.join_comma_separated(item.end())
                    .nodes(elts.iter())
                    .finish()
            }
        });

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        parenthesized("{", &joined, "}")
            .with_dangling_comments(dangling)
            .fmt(f)
    }
}

impl NeedsParentheses for ExprSet {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}
