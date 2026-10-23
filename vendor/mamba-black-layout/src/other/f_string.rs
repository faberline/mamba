use super::interpolated_string_element::FormatInterpolatedStringElement;
use crate::context::WithInterpolatedStringState;
use crate::other::interpolated_string::{InterpolatedStringContext, InterpolatedStringLayout};
use crate::prelude::*;
use crate::string::{StringNormalizer, StringQuotes};
use ruff_formatter::write;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{FString, InterpolatedStringElements, StringFlags};
use ruff_text_size::Ranged;

pub(crate) fn has_skipped_interpolation_comment(
    elements: &InterpolatedStringElements,
    f: &PyFormatter,
    source: &str,
) -> bool {
    elements.interpolations().any(|element| {
        interpolation_expression_is_multiline(element, f, source)
            && expression_has_skip_comment(&element.expression, f, source)
    })
}

fn has_completed_skip_interpolation(
    elements: &InterpolatedStringElements,
    f: &PyFormatter,
    source: &str,
) -> bool {
    elements.interpolations().any(|element| {
        !interpolation_expression_is_multiline(element, f, source)
            && expression_has_skip_comment(&element.expression, f, source)
    })
}

fn interpolation_expression_is_multiline(
    element: &ruff_python_ast::InterpolatedElement,
    f: &PyFormatter,
    source: &str,
) -> bool {
    let expression = &element.expression;
    if source[expression.range()].contains('\n') {
        return true;
    }

    use ruff_python_ast::token::TokenKind;

    let interpolation = element.range();
    let expression = expression.range();
    let string_start =
        interpolated_string_start(interpolation.start(), f).unwrap_or(interpolation.start());
    let mut delimiters = Vec::new();
    for token in f
        .context()
        .tokens()
        .iter()
        .filter(|token| token.start() >= string_start && token.end() <= interpolation.end())
    {
        match token.kind() {
            TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace => {
                delimiters.push((token.kind(), token.start()));
            }
            TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace => {
                let expected = match token.kind() {
                    TokenKind::Rpar => TokenKind::Lpar,
                    TokenKind::Rsqb => TokenKind::Lsqb,
                    TokenKind::Rbrace => TokenKind::Lbrace,
                    _ => unreachable!(),
                };
                if let Some(index) = delimiters.iter().rposition(|(kind, _)| *kind == expected) {
                    let (kind, start) = delimiters.remove(index);
                    if kind == TokenKind::Lpar
                        && (start < expression.start() && expression.end() < token.end()
                            || expression.start() < start && token.end() < expression.end())
                        && source[start.to_usize()..token.end().to_usize()].contains('\n')
                    {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }
    false
}

fn expression_has_skip_comment(
    expression: &ruff_python_ast::Expr,
    f: &PyFormatter,
    source: &str,
) -> bool {
    struct ContainsSkip<'comments, 'source, 'ast> {
        comments: &'comments crate::comments::Comments<'ast>,
        source: &'source str,
        found: bool,
    }

    impl<'ast> ruff_python_ast::visitor::Visitor<'ast> for ContainsSkip<'_, '_, 'ast> {
        fn visit_expr(&mut self, expression: &'ast ruff_python_ast::Expr) {
            self.found |=
                crate::comments::has_skip_comment(self.comments.trailing(expression), self.source);
            if !self.found {
                ruff_python_ast::visitor::walk_expr(self, expression);
            }
        }
    }

    let mut visitor = ContainsSkip {
        comments: f.context().comments(),
        source,
        found: false,
    };
    visitor.visit_expr(expression);
    visitor.found
}

pub(crate) fn black_skipped_interpolation_candidate(
    elements: &InterpolatedStringElements,
    f: &PyFormatter,
    source: &str,
) -> Option<String> {
    let formatted_source = f.context().source();
    let comment_start = skipped_interpolation_comment_start(elements, formatted_source, source, f)?;

    let mut line_start = 0;
    for line in source.split_inclusive('\n') {
        if line_start == comment_start {
            let content = line.strip_suffix('\n').unwrap_or(line);
            let next_line_start = line_start + line.len();
            let next_line_end = source[next_line_start..]
                .find('\n')
                .map_or(source.len(), |offset| next_line_start + offset);
            let mut candidate = String::with_capacity(source.len());
            candidate.push_str(&source[..line_start]);
            candidate.push_str(content);
            candidate.push_str(&source[next_line_start..next_line_end]);
            candidate.push_str(&source[next_line_end..]);
            return Some(candidate);
        }
        line_start += line.len();
    }
    None
}

fn skipped_interpolation_comment_start(
    elements: &InterpolatedStringElements,
    formatted_source: &str,
    source: &str,
    f: &PyFormatter,
) -> Option<usize> {
    struct SkipComments<'comments, 'source, 'ast> {
        comments: &'comments crate::comments::Comments<'ast>,
        source: &'source str,
        ranges: Vec<ruff_text_size::TextRange>,
    }

    impl<'ast> ruff_python_ast::visitor::Visitor<'ast> for SkipComments<'_, '_, 'ast> {
        fn visit_expr(&mut self, expression: &'ast ruff_python_ast::Expr) {
            for comment in self.comments.trailing(expression) {
                let range = comment.range();
                if comment.line_position().is_end_of_line()
                    && crate::comments::suppression::controls(&self.source[range])
                        .contains(&ruff_python_trivia::SuppressionKind::Skip)
                {
                    self.ranges.push(range);
                }
            }
            ruff_python_ast::visitor::walk_expr(self, expression);
        }
    }

    for element in elements.interpolations() {
        let expression = &element.expression;
        let range = expression.range();
        if !interpolation_expression_is_multiline(element, f, formatted_source) {
            continue;
        }
        let Some(string_start) = interpolated_string_start(element.range.start(), f) else {
            continue;
        };

        let mut visitor = SkipComments {
            comments: f.context().comments(),
            source: formatted_source,
            ranges: Vec::new(),
        };
        visitor.visit_expr(expression.as_ref());

        for comment in visitor.ranges {
            if comment.start() < range.start() {
                continue;
            }
            let line_delta = formatted_source[string_start.to_usize()..comment.start().to_usize()]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count();
            let mut line_start = 0;
            for _ in 0..line_delta {
                let newline = source[line_start..].find('\n')?;
                line_start += newline + 1;
            }
            let line_end = source[line_start..]
                .find('\n')
                .map_or(source.len(), |offset| line_start + offset);
            let line = &source[line_start..line_end];
            let comment_text = formatted_source[comment].trim_end();
            if line.rfind(comment_text).is_some_and(|hash| {
                crate::comments::suppression::controls(&line[hash..])
                    .contains(&ruff_python_trivia::SuppressionKind::Skip)
            }) {
                return Some(line_start);
            }
        }
    }
    None
}

fn interpolated_string_start(
    position: ruff_text_size::TextSize,
    f: &PyFormatter,
) -> Option<ruff_text_size::TextSize> {
    use ruff_python_ast::token::TokenKind;

    let mut strings = Vec::new();
    for token in f
        .context()
        .tokens()
        .iter()
        .take_while(|token| token.start() < position)
    {
        match token.kind() {
            TokenKind::FStringStart | TokenKind::TStringStart => {
                strings.push((token.kind(), token.start()));
            }
            TokenKind::FStringEnd | TokenKind::TStringEnd => {
                strings.pop();
            }
            _ => {}
        }
    }
    strings.last().map(|(_, start)| *start)
}

fn is_enclosed_by_parentheses(range: ruff_text_size::TextRange, f: &PyFormatter) -> bool {
    use ruff_python_ast::token::TokenKind;

    let mut delimiters = Vec::new();
    for token in f
        .context()
        .tokens()
        .iter()
        .take_while(|token| token.start() < range.start())
    {
        match token.kind() {
            TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace => delimiters.push(token.kind()),
            TokenKind::Rpar => {
                if delimiters.pop() != Some(TokenKind::Lpar) {
                    return false;
                }
            }
            TokenKind::Rsqb => {
                if delimiters.pop() != Some(TokenKind::Lsqb) {
                    return false;
                }
            }
            TokenKind::Rbrace => {
                if delimiters.pop() != Some(TokenKind::Lbrace) {
                    return false;
                }
            }
            _ => {}
        }
    }
    delimiters.contains(&TokenKind::Lpar)
}

/// Formats an f-string which is part of a larger f-string expression.
///
/// For example, this would be used to format the f-string part in `"foo" f"bar {x}"`
/// or the standalone f-string in `f"foo {x} bar"`.
#[derive(Default)]
pub struct FormatFString;

impl FormatNodeRule<FString> for FormatFString {
    fn fmt_fields(&self, item: &FString, f: &mut PyFormatter) -> FormatResult<()> {
        if let Some(policy) = f.options().style_policy() {
            let source = f.context().source();
            let backslash = item
                .elements
                .interpolations()
                .any(|part| source[part.range()].contains('\\'));
            let normalized = policy.interpolated(&source[item.range()], backslash);
            if has_completed_skip_interpolation(&item.elements, f, source)
                && is_enclosed_by_parentheses(item.range(), f)
            {
                expand_parent().fmt(f)?;
            }
            let candidate = black_skipped_interpolation_candidate(&item.elements, f, &normalized)
                .unwrap_or(normalized);
            f.context()
                .comments()
                .mark_verbatim_node_comments_formatted(item.into());
            return text(&candidate).fmt(f);
        }
        let normalizer = StringNormalizer::from_context(f.context());

        let string_kind = normalizer.choose_quotes(item.into()).flags();

        let context = InterpolatedStringContext::new(
            string_kind,
            InterpolatedStringLayout::from_interpolated_string_elements(
                &item.elements,
                f.context().source(),
            ),
        );

        // Starting prefix and quote
        let quotes = StringQuotes::from(string_kind);
        write!(f, [string_kind.prefix(), quotes])?;

        {
            let state = f
                .context()
                .interpolated_string_state()
                .enter_string(context);
            let f = &mut WithInterpolatedStringState::new(state, &mut *f);
            for element in &item.elements {
                FormatInterpolatedStringElement::new(element, context).fmt(f)?;
            }
        }

        // Ending quote
        quotes.fmt(f)
    }
}
