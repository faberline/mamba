use ruff_formatter::{FormatRuleWithOptions, format_args, write};
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprGenerator;
use ruff_python_ast::token::{Tokens, parentheses_iterator};

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses, parenthesized};
use crate::prelude::*;
use crate::{builders::parenthesize_if_expands, expression::parentheses::Parentheses};

#[derive(Eq, PartialEq, Debug, Default)]
pub enum GeneratorExpParentheses {
    #[default]
    Default,

    /// Skips the parentheses if they aren't present in the source code. Used when formatting call expressions
    /// because the parentheses are optional if the generator is the **only** argument:
    ///
    /// ```python
    /// all(x for y in z)`
    /// ```
    Preserve,

    /// Black's stable mode keeps each extra pair in calls and expression statements.
    Nested(usize),
}

pub(crate) fn black_parentheses(
    generator: &ExprGenerator,
    parent: AnyNodeRef,
    tokens: &Tokens,
) -> GeneratorExpParentheses {
    GeneratorExpParentheses::Nested(
        parentheses_iterator(generator.into(), Some(parent), tokens).count(),
    )
}

impl FormatRuleWithOptions<ExprGenerator, PyFormatContext<'_>> for FormatExprGenerator {
    type Options = GeneratorExpParentheses;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.parentheses = options;
        self
    }
}

#[derive(Default)]
pub struct FormatExprGenerator {
    parentheses: GeneratorExpParentheses,
}

impl FormatNodeRule<ExprGenerator> for FormatExprGenerator {
    fn fmt_fields(&self, item: &ExprGenerator, f: &mut PyFormatter) -> FormatResult<()> {
        if super::black_comprehension::format(&item.generators, f, |f| self.fmt_fields(item, f))? {
            return Ok(());
        }
        if let GeneratorExpParentheses::Nested(count) = self.parentheses
            && count > 0
        {
            return parenthesized(
                "(",
                &format_with(|f| {
                    FormatExprGenerator {
                        parentheses: GeneratorExpParentheses::Nested(count - 1),
                    }
                    .fmt_fields(item, f)
                }),
                ")",
            )
            .fmt(f);
        }
        let ExprGenerator {
            range: _,
            node_index: _,
            elt,
            generators,
            parenthesized: is_parenthesized,
        } = item;
        let formatted_elt = format_with(|f: &mut PyFormatter| {
            if f.options().style_policy().is_some() && elt.is_if_expr() {
                parenthesize_if_expands(&elt.format().with_options(Parentheses::Never)).fmt(f)
            } else {
                elt.format().fmt(f)
            }
        });

        let joined = format_with(|f| super::black_comprehension::generators(generators, f));

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        if matches!(
            self.parentheses,
            GeneratorExpParentheses::Preserve | GeneratorExpParentheses::Nested(0)
        ) && dangling.is_empty()
            && !is_parenthesized
        {
            write!(
                f,
                [group(&formatted_elt), soft_line_break_or_space(), &joined]
            )
        } else {
            write!(
                f,
                [parenthesized(
                    "(",
                    &group(&format_args!(
                        group(&formatted_elt),
                        soft_line_break_or_space(),
                        joined
                    )),
                    ")"
                )
                .with_dangling_comments(dangling)]
            )
        }
    }
}

impl NeedsParentheses for ExprGenerator {
    fn needs_parentheses(
        &self,
        parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        if parent.is_expr_await() {
            OptionalParentheses::Always
        } else {
            OptionalParentheses::Never
        }
    }
}
