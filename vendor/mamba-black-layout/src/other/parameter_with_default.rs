use ruff_formatter::{FormatRuleWithOptions, write};
use ruff_python_ast::ParameterWithDefault;
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextRange};

use crate::prelude::*;
use crate::{builders::parenthesize_if_expands, expression::parentheses::Parentheses};
pub(crate) mod black;

#[derive(Default)]
pub struct FormatParameterWithDefault {
    lambda: bool,
}

impl FormatRuleWithOptions<ParameterWithDefault, PyFormatContext<'_>>
    for FormatParameterWithDefault
{
    type Options = bool;
    fn with_options(mut self, lambda: bool) -> Self {
        self.lambda = lambda;
        self
    }
}

impl FormatNodeRule<ParameterWithDefault> for FormatParameterWithDefault {
    fn fmt_fields(&self, item: &ParameterWithDefault, f: &mut PyFormatter) -> FormatResult<()> {
        let ParameterWithDefault {
            range: _,
            node_index: _,
            parameter,
            default,
        } = item;

        write!(f, [parameter.format()])?;

        if let Some(default) = default {
            let space = parameter.annotation.is_some().then_some(space());
            // ```python
            // def f(
            //     a = # parameter trailing comment; needs line break
            //     1,
            //     b =
            //     # default leading comment; needs line break
            //     2,
            //     c = ( # the default leading can only be end-of-line with parentheses; no line break
            //         3
            //     ),
            //     d = (
            //         # own line leading comment with parentheses; no line break
            //         4
            //     )
            // )
            // ```
            let needs_line_break_trailing = f.context().comments().has_trailing(parameter);
            let default_first_comment = f.context().comments().leading(default.as_ref()).first();
            let needs_line_break_leading =
                default_first_comment.is_some_and(|default_leading_comment| {
                    let mut tokenizer = SimpleTokenizer::new(
                        f.context().source(),
                        TextRange::new(parameter.end(), default_leading_comment.start()),
                    )
                    .skip_trivia()
                    .skip_while(|token| token.kind == SimpleTokenKind::RParen);
                    let equals = tokenizer.next();
                    debug_assert!(
                        equals.is_some_and(|token| token.kind == SimpleTokenKind::Equals)
                    );
                    let lparens = tokenizer.next();
                    debug_assert!(
                        lparens
                            .as_ref()
                            .is_none_or(|token| token.kind == SimpleTokenKind::LParen)
                    );
                    lparens.is_none()
                });
            let needs_line_break = needs_line_break_trailing || needs_line_break_leading;
            let formatted = format_with(|f: &mut PyFormatter| {
                if self.lambda && black::format(default, parameter.end(), f)? {
                    Ok(())
                } else if f.options().style_policy().is_some() && default.is_if_expr() {
                    parenthesize_if_expands(&default.format().with_options(Parentheses::Never))
                        .fmt(f)
                } else {
                    default.format().fmt(f)
                }
            });

            write!(
                f,
                [
                    space,
                    token("="),
                    (!needs_line_break).then_some(space),
                    needs_line_break.then_some(hard_line_break()),
                    formatted
                ]
            )?;
        }

        Ok(())
    }
}
