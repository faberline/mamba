use ruff_formatter::{FormatOptions, format_args, write};
use ruff_python_ast::{AnyNodeRef, DictItem, Expr, ExprDict, StringLike};
use ruff_text_size::{Ranged, TextRange};

use crate::comments::{SourceComment, dangling_comments, leading_comments};
use crate::expression::parentheses::{
    NeedsParentheses, OptionalParentheses, empty_parenthesized, parenthesized,
};
use crate::prelude::*;
mod black;

#[derive(Default)]
pub struct FormatExprDict;

impl FormatNodeRule<ExprDict> for FormatExprDict {
    fn fmt_fields(&self, item: &ExprDict, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprDict {
            range: _,
            node_index: _,
            items,
        } = item;

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        let Some(first_dict_item) = items.first() else {
            return empty_parenthesized("{", dangling, "}").fmt(f);
        };

        // Dangling comments can either appear after the open bracket, or around the key-value
        // pairs:
        // ```python
        // {  # open_parenthesis_comments
        //     x:  # key_value_comments
        //     y
        // }
        // ```
        let (open_parenthesis_comments, key_value_comments) =
            dangling.split_at(dangling.partition_point(|comment| {
                comment.end() < KeyValuePair::new(first_dict_item).start()
            }));

        let format_pairs = format_with(|f: &mut PyFormatter| {
            if f.options().style_policy().is_some()
                && StringLike::try_from(&items.last().unwrap().value).is_ok_and(|string| {
                    string.parts().last().is_some_and(|part| {
                        let normalized = f
                            .options()
                            .style_policy()
                            .unwrap()
                            .literal(&f.context().source()[part.range()]);
                        normalized.contains('\n')
                            && normalized.rsplit('\n').next().is_some_and(|last| {
                                ruff_formatter::black_width::measure(last) + 1
                                    > usize::from(f.options().line_width().value())
                            })
                    })
                })
            {
                expand_parent().fmt(f)?;
            }
            if f.options().style_policy().is_some()
                && items.len() == 1
                && !f
                    .context()
                    .is_expression_parenthesized((&items[0].value).into())
                && items[0].value.as_lambda_expr().is_some_and(|lambda| {
                    !f.context()
                        .is_expression_parenthesized(lambda.body.as_ref().into())
                        && crate::black_multiline::contains(&lambda.body, f.context().source())
                })
            {
                expand_parent().fmt(f)?;
            }
            if f.options().style_policy().is_some()
                && items.len() > 1
                && items.iter().any(|item| {
                    crate::black_multiline::contains(&item.value, f.context().source())
                        || item.key.as_ref().is_some_and(|key| {
                            crate::black_multiline::contains(key, f.context().source())
                        })
                })
            {
                expand_parent().fmt(f)?;
            }
            let mut joiner = f.join_comma_separated(item.end());

            let mut key_value_comments = key_value_comments;
            for dict_item in items {
                let mut key_value_pair = KeyValuePair::new(dict_item);

                let partition = key_value_comments
                    .partition_point(|comment| comment.start() < key_value_pair.end());
                key_value_pair = key_value_pair.with_comments(&key_value_comments[..partition]);
                key_value_comments = &key_value_comments[partition..];

                joiner.entry(&key_value_pair, &key_value_pair);
            }

            joiner.finish()
        });

        parenthesized("{", &format_pairs, "}")
            .with_dangling_comments(open_parenthesis_comments)
            .fmt(f)
    }
}

impl NeedsParentheses for ExprDict {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}

#[derive(Debug)]
struct KeyValuePair<'a> {
    key: &'a Option<Expr>,
    value: &'a Expr,
    comments: &'a [SourceComment],
}

impl<'a> KeyValuePair<'a> {
    fn new(item: &'a DictItem) -> Self {
        Self {
            key: &item.key,
            value: &item.value,
            comments: &[],
        }
    }

    fn with_comments(self, comments: &'a [SourceComment]) -> Self {
        Self { comments, ..self }
    }
}

impl Ranged for KeyValuePair<'_> {
    fn range(&self) -> TextRange {
        if let Some(key) = self.key {
            TextRange::new(key.start(), self.value.end())
        } else {
            self.value.range()
        }
    }
}

impl Format<PyFormatContext<'_>> for KeyValuePair<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        if f.context().off_ranges().contains(self.range())
            || crate::black_suppression::line::skip_after(self.range(), f.context())
        {
            let comments = f.context().comments().clone();
            if let Some(key) = self.key {
                leading_comments(comments.leading(key)).fmt(f)?;
                crate::black_suppression::line::mark(key.into(), self.range(), &comments);
            } else {
                leading_comments(comments.leading(self.value)).fmt(f)?;
            }
            crate::black_suppression::line::mark(self.value.into(), self.range(), &comments);
            if f.context().off_ranges().contains(self.range()) {
                crate::black_suppression::off::write_range(self.range(), f)?;
            } else {
                crate::verbatim::verbatim_text(self.range()).fmt(f)?;
            }
            return crate::comments::trailing_comments(comments.trailing(self.value)).fmt(f);
        }
        if let Some(key) = self.key {
            if black::format_pair(key, self.value, self.comments, f)? {
                return Ok(());
            }
            write!(
                f,
                [group(&format_with(|f| {
                    black::format_key(key, f)?;
                    token(":").fmt(f)?;

                    if self.comments.is_empty() {
                        space().fmt(f)?;
                    } else {
                        dangling_comments(self.comments).fmt(f)?;
                    }

                    if f.options().style_policy().is_some()
                        && f.context().is_expression_parenthesized(self.value.into())
                        && self.value.is_string_literal_expr()
                        && crate::black_multiline::contains(self.value, f.context().source())
                    {
                        write!(
                            f,
                            [
                                token("("),
                                block_indent(&self.value.format().with_options(
                                    crate::expression::parentheses::Parentheses::Never
                                )),
                                token(")")
                            ]
                        )
                    } else {
                        self.value.format().fmt(f)
                    }
                }))]
            )
        } else {
            // TODO(charlie): Make these dangling comments on the `ExprDict`, and identify them
            // dynamically, so as to avoid the parent rendering its child's comments.
            let comments = f.context().comments().clone();
            let leading_value_comments = comments.leading(self.value);
            write!(
                f,
                [
                    // make sure the leading comments are hoisted past the `**`
                    leading_comments(leading_value_comments),
                    group(&format_args![token("**"), self.value.format()])
                ]
            )
        }
    }
}
