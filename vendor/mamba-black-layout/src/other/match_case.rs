use ruff_formatter::{FormatRuleWithOptions, format_args, write};
use ruff_python_ast::{Expr, MatchCase};

use crate::context::{NodeLevel, WithNodeLevel};
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::{Parentheses, Parenthesize};
use crate::pattern::maybe_parenthesize_pattern;
use crate::prelude::*;
use crate::statement::clause::{ClauseHeader, clause};
use crate::statement::suite::SuiteKind;

#[derive(Default)]
pub struct FormatMatchCase {
    last_suite_in_statement: bool,
}

impl FormatRuleWithOptions<MatchCase, PyFormatContext<'_>> for FormatMatchCase {
    type Options = bool;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.last_suite_in_statement = options;
        self
    }
}

impl FormatNodeRule<MatchCase> for FormatMatchCase {
    fn fmt_fields(&self, item: &MatchCase, f: &mut PyFormatter) -> FormatResult<()> {
        let MatchCase {
            range: _,
            node_index: _,
            pattern,
            guard,
            body,
        } = item;

        let comments = f.context().comments().clone();
        let dangling_item_comments = comments.dangling(item);

        let header_id = f.group_id("black_match_guard");
        let format_guard = guard.as_deref().map(|guard| {
            format_with(|f: &mut PyFormatter| {
                write!(f, [space(), token("if"), space()])?;

                if f.options().style_policy().is_some()
                    && matches!(
                        guard,
                        Expr::BinOp(_) | Expr::BoolOp(_) | Expr::Compare(_) | Expr::If(_)
                    )
                {
                    let mut f = WithNodeLevel::new(NodeLevel::ParenthesizedExpression, f);
                    format_args![
                        if_group_breaks(&token("(")).with_group_id(Some(header_id)),
                        indent_if_group_breaks(
                            &format_args![
                                soft_line_break(),
                                guard.format().with_options(Parentheses::Never)
                            ],
                            header_id
                        ),
                        soft_line_break(),
                        if_group_breaks(&token(")")).with_group_id(Some(header_id))
                    ]
                    .fmt(&mut f)
                } else {
                    maybe_parenthesize_expression(guard, item, Parenthesize::IfBreaksParenthesized)
                        .fmt(f)
                }
            })
        });

        write!(
            f,
            [clause(
                ClauseHeader::MatchCase(item),
                &group(&format_args![
                    token("case"),
                    space(),
                    maybe_parenthesize_pattern(pattern, item),
                    format_guard
                ])
                .with_id(Some(header_id)),
                dangling_item_comments,
                body,
                SuiteKind::other(self.last_suite_in_statement),
            )]
        )
    }
}
