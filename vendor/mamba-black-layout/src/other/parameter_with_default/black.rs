//! Standalone opening comments in lambda defaults use the statement's body indentation.
use crate::prelude::*;
use ruff_formatter::format_element::tag::DedentMode;
use ruff_formatter::{FormatContext, FormatOptions};
use ruff_python_ast::{Expr, token::TokenKind};
use ruff_text_size::{Ranged, TextRange, TextSize};

fn opening_comment(range: TextRange, context: &PyFormatContext<'_>) -> bool {
    let mut equal = false;
    let mut opening = None;
    let mut last_code = None;
    let mut depth = 0i32;
    for item in context.tokens().in_range(range) {
        let kind = item.kind();
        if kind == TokenKind::Comment {
            let start = usize::from(item.start());
            let line = context.source()[..start]
                .rsplit(['\n', '\r'])
                .next()
                .unwrap_or("");
            if opening.is_some() && opening == last_code && line.trim().is_empty() {
                return true;
            }
            continue;
        }
        if kind.is_trivia() || kind.is_any_newline() {
            continue;
        }
        if kind == TokenKind::Colon && depth == 0 {
            break;
        }
        if kind == TokenKind::Equal && depth == 0 {
            equal = true;
        }
        if matches!(kind, TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace) {
            depth += 1;
            if equal && opening.is_none() {
                opening = Some(item.start());
            }
        } else if matches!(kind, TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace) {
            depth -= 1;
        }
        last_code = Some(item.start());
    }
    false
}

pub(crate) fn lambda_comments(range: TextRange, context: &PyFormatContext<'_>) -> bool {
    if context.options().style_policy().is_none() {
        return false;
    }
    let mut depth = 0i32;
    let mut value_start = true;
    let mut lambda = None;
    for item in context.tokens().in_range(range) {
        let kind = item.kind();
        if kind.is_trivia() || kind.is_any_newline() {
            continue;
        }
        if kind == TokenKind::Lambda && depth == 0 && value_start {
            lambda = Some(item);
            break;
        }
        if matches!(kind, TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace) {
            depth += 1;
        } else if matches!(kind, TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace) {
            depth -= 1;
        }
        value_start = depth == 0 && matches!(kind, TokenKind::Equal | TokenKind::Colon);
    }
    lambda.is_some_and(|item| opening_comment(TextRange::new(item.end(), range.end()), context))
}

pub(super) fn format(
    default: &Expr,
    parameter_end: TextSize,
    f: &mut PyFormatter,
) -> FormatResult<bool> {
    if f.options().style_policy().is_none()
        || !opening_comment(TextRange::new(parameter_end, default.end()), f.context())
    {
        return Ok(false);
    }
    let formatted = default.format().memoized();
    let mut flattened = Vec::new();
    fn flatten(items: &[FormatElement], output: &mut Vec<FormatElement>) {
        for item in items {
            if let FormatElement::Interned(items) = item {
                flatten(items, output);
            } else {
                output.push(item.clone());
            }
        }
    }
    flatten(formatted.inspect(f)?, &mut flattened);
    let width = f.options().indent_width();
    let levels = u32::from(f.context().indent_level().to_ascii_spaces(width)) / width.value() + 1;
    let mut root = false;
    let mut inside = 0usize;
    for item in flattened {
        match item {
            FormatElement::Tag(Tag::StartIndent) if !root => {
                root = true;
                inside = 1;
                f.write_element(FormatElement::Tag(Tag::StartDedent(DedentMode::Root)));
                for _ in 0..levels {
                    f.write_element(FormatElement::Tag(Tag::StartIndent));
                }
            }
            FormatElement::Tag(Tag::StartIndent) if inside > 0 => {
                inside += 1;
                f.write_element(item);
            }
            FormatElement::Tag(Tag::EndIndent) if inside > 0 => {
                inside -= 1;
                if inside == 0 {
                    for _ in 0..levels {
                        f.write_element(FormatElement::Tag(Tag::EndIndent));
                    }
                    f.write_element(FormatElement::Tag(Tag::EndDedent));
                } else {
                    f.write_element(item);
                }
            }
            _ => f.write_element(item),
        }
    }
    Ok(true)
}
