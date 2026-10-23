use ruff_formatter::{FormatContext, FormatRuleWithOptions, RemoveSoftLinesBuffer, write};
use ruff_python_ast::{ArgOrKeyword, Arguments, Expr, StringFlags, StringLike};
use ruff_python_trivia::{PythonWhitespace, SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::expression::expr_generator::{GeneratorExpParentheses, black_parentheses};
use crate::expression::is_expression_huggable;
use crate::expression::parentheses::{Parentheses, empty_parenthesized, parenthesized};
use crate::other::commas;
use crate::prelude::*;
use crate::string::StringLikeExtensions;

#[derive(Default)]
pub struct FormatArguments {
    force_split: bool,
}

impl FormatRuleWithOptions<Arguments, PyFormatContext<'_>> for FormatArguments {
    type Options = bool;

    fn with_options(mut self, options: bool) -> Self {
        self.force_split = options;
        self
    }
}

impl FormatNodeRule<Arguments> for FormatArguments {
    fn fmt_fields(&self, item: &Arguments, f: &mut PyFormatter) -> FormatResult<()> {
        let Arguments {
            range,
            args,
            keywords,
            node_index: _,
        } = item;
        // We have a case with `f()` without any argument, which is a special case because we can
        // have a comment with no node attachment inside:
        // ```python
        // f(
        //      # This call has a dangling comment.
        // )
        // ```
        if args.is_empty() && keywords.is_empty() {
            let comments = f.context().comments().clone();
            let dangling = comments.dangling(item);
            return write!(f, [empty_parenthesized("(", dangling, ")")]);
        }

        let all_arguments = format_with(|f: &mut PyFormatter| {
            let source = f.context().source();
            if f.options().style_policy().is_some()
                && item.len() > 1
                && item
                    .iter_source_order()
                    .any(|argument| crate::black_multiline::contains(argument.value(), source))
            {
                expand_parent().fmt(f)?;
            }
            let native = f.options().style_policy().is_some();
            let tokens = f.context().tokens();
            let conditional_arguments: Vec<_> = item
                .iter_source_order()
                .filter_map(|argument| match argument {
                    ArgOrKeyword::Arg(arg)
                        if native
                            && arg.is_if_expr()
                            && !f.context().is_expression_parenthesized(arg.into()) =>
                    {
                        Some(arg.range())
                    }
                    _ => None,
                })
                .collect();
            let mut joiner = f.join_comma_separated(range.end());
            match args.as_ref() {
                [arg] if keywords.is_empty() => {
                    match arg {
                        Expr::Generator(generator_exp) => joiner.entry(
                            generator_exp,
                            &generator_exp.format().with_options(if native {
                                black_parentheses(generator_exp, item.into(), tokens)
                            } else {
                                GeneratorExpParentheses::Preserve
                            }),
                        ),
                        other => {
                            if conditional_arguments.contains(&other.range()) {
                                joiner.entry(
                                    other,
                                    &crate::builders::parenthesize_if_expands(
                                        &other.format().with_options(Parentheses::Never),
                                    ),
                                );
                                return joiner.finish();
                            }
                            let parentheses =
                                if is_single_argument_parenthesized(arg, range.end(), source) {
                                    Parentheses::Always
                                } else {
                                    // Note: no need to handle opening-parenthesis comments, since
                                    // an opening-parenthesis comment implies that the argument is
                                    // parenthesized.
                                    Parentheses::Never
                                };
                            joiner.entry(other, &other.format().with_options(parentheses))
                        }
                    };
                }
                _ => {
                    for arg_or_keyword in item.iter_source_order() {
                        match arg_or_keyword {
                            ArgOrKeyword::Arg(arg) => {
                                if let Expr::Generator(generator) = arg
                                    && native
                                {
                                    joiner.entry(
                                        arg,
                                        &generator.format().with_options(black_parentheses(
                                            generator,
                                            item.into(),
                                            tokens,
                                        )),
                                    );
                                } else if conditional_arguments.contains(&arg.range()) {
                                    joiner.entry(
                                        arg,
                                        &crate::builders::parenthesize_if_expands(
                                            &arg.format().with_options(Parentheses::Never),
                                        ),
                                    );
                                } else {
                                    joiner.entry(arg, &arg.format());
                                }
                            }
                            ArgOrKeyword::Keyword(keyword) => {
                                joiner.entry(keyword, &keyword.format());
                            }
                        }
                    }
                }
            }

            joiner.finish()
        });

        // If the arguments are non-empty, then a dangling comment indicates a comment on the
        // same line as the opening parenthesis, e.g.:
        // ```python
        // f(  # This call has a dangling comment.
        //     a,
        //     b,
        //     c,
        // )
        let comments = f.context().comments().clone();
        let dangling_comments = comments.dangling(item);

        let mut source_order = item.iter_source_order();
        let first = source_order.next().unwrap();
        let last = source_order.last().unwrap_or(first);
        let last_node = match last {
            ArgOrKeyword::Arg(arg) => arg.into(),
            ArgOrKeyword::Keyword(keyword) => keyword.into(),
        };
        let keep_argument_line = crate::black_comments::unsplittable(last_node, f.context())
            && !f.context().source()[TextRange::new(first.start(), last.end())]
                .contains(['\r', '\n'])
            && !commas::has_magic_trailing_comma(
                TextRange::new(last.end(), item.end()),
                f.context(),
            );
        let inner = format_with(|f| {
            if keep_argument_line {
                let mut buffer = RemoveSoftLinesBuffer::new(f);
                write!(buffer, [all_arguments])
            } else {
                all_arguments.fmt(f)
            }
        });

        write!(
            f,
            [
                // The outer group is for things like:
                // ```python
                // get_collection(
                //     hey_this_is_a_very_long_call,
                //     it_has_funny_attributes_asdf_asdf,
                //     too_long_for_the_line,
                //     really=True,
                // )
                // ```
                // The inner group is for things like:
                // ```python
                // get_collection(
                //     hey_this_is_a_very_long_call, it_has_funny_attributes_asdf_asdf, really=True
                // )
                // ```
                parenthesized("(", &group(&inner), ")")
                    .with_force_expansion(self.force_split || f.context().bracket_is_forced(*range))
                    .with_hugging(is_arguments_huggable(item, f.context()))
                    .with_dangling_comments(dangling_comments)
            ]
        )
    }
}

fn is_single_argument_parenthesized(argument: &Expr, call_end: TextSize, source: &str) -> bool {
    let mut has_seen_r_paren = false;

    for token in
        SimpleTokenizer::new(source, TextRange::new(argument.end(), call_end)).skip_trivia()
    {
        match token.kind() {
            SimpleTokenKind::RParen => {
                if has_seen_r_paren {
                    return true;
                }
                has_seen_r_paren = true;
            }
            // Skip over any trailing comma
            SimpleTokenKind::Comma => continue,
            _ => {
                // Passed the arguments
                break;
            }
        }
    }

    false
}

/// Returns `true` if the arguments can hug directly to the enclosing parentheses in the call, as
/// in Black's `hug_parens_with_braces_and_square_brackets` preview style behavior.
///
/// For example, in preview style, given:
/// ```python
/// func([1, 2, 3,])
/// ```
///
/// We want to format it as:
/// ```python
/// func([
///     1,
///     2,
///     3,
/// ])
/// ```
///
/// As opposed to:
/// ```python
/// func(
///     [
///         1,
///         2,
///         3,
///     ]
/// )
/// ```
///
/// Hugging should only be applied to single-argument collections, like lists, or starred versions
/// of those collections.
fn is_arguments_huggable(arguments: &Arguments, context: &PyFormatContext) -> bool {
    // Find the lone argument or `**kwargs` keyword.
    let arg = match (arguments.args.as_ref(), arguments.keywords.as_ref()) {
        ([arg], []) => arg,
        ([], [keyword]) if keyword.arg.is_none() && !context.comments().has(keyword) => {
            &keyword.value
        }
        _ => return false,
    };

    // If the expression itself isn't huggable, then we can't hug it.
    if !(is_expression_huggable(arg, context)
        || StringLike::try_from(arg)
            .is_ok_and(|string| is_huggable_string_argument(string, arguments, context)))
    {
        return false;
    }

    // If the expression has leading or trailing comments, then we can't hug it.
    let comments = context.comments().leading_dangling_trailing(arg);
    if comments.has_leading() || comments.has_trailing() {
        return false;
    }

    let options = context.options();

    // If the expression has a trailing comma, then we can't hug it.
    if options.magic_trailing_comma().is_respect()
        && commas::has_magic_trailing_comma(TextRange::new(arg.end(), arguments.end()), context)
    {
        return false;
    }

    true
}

/// Returns `true` if `string` is a multiline string that is not implicitly concatenated and there's no
/// newline between the opening parentheses of arguments and the quotes of the string:
///
/// ```python
/// # Hug this string
/// call("""test
/// multiline""")
///
/// # Don't hug because there's a newline between the opening parentheses and the quotes:
/// call(
///     """"
///     test
///     """"
/// )
/// ```
fn is_huggable_string_argument(
    string: StringLike,
    arguments: &Arguments,
    context: &PyFormatContext,
) -> bool {
    if string.is_implicit_concatenated()
        || !string.is_multiline(context)
        || !string
            .parts()
            .next()
            .is_some_and(|part| part.flags().is_triple_quoted())
    {
        return false;
    }

    let between_parens_range = TextRange::new(arguments.start() + '('.text_len(), string.start());
    let between_parens = &context.source()[between_parens_range];
    !between_parens.trim_whitespace_end().ends_with(['\n', '\r'])
}
