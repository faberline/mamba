//! Token ranges for Black's own-line off/on instructions.
use crate::prelude::*;
use ruff_formatter::format_args;
use ruff_python_ast::{
    AnyNodeRef,
    token::{TokenKind, Tokens},
};
use ruff_python_trivia::{CommentLinePosition, SuppressionKind};
use ruff_text_size::{Ranged, TextRange, TextSize};
pub(crate) mod comments;

#[derive(Debug, Default)]
pub(crate) struct OffRanges(Vec<TextRange>);

impl OffRanges {
    pub(crate) fn end_at(&self, position: TextSize) -> Option<TextSize> {
        let index = self.0.partition_point(|region| region.end() <= position);
        self.0
            .get(index)
            .filter(|region| region.contains_range(TextRange::empty(position)))
            .map(Ranged::end)
    }

    pub(crate) fn collect(source: &str, tokens: &Tokens) -> Self {
        let mut ranges = Vec::new();
        let mut active: Option<(TextSize, u32, u32)> = None;
        let mut depth = 0u32;
        let mut indentation = 0u32;
        let mut columns = vec![0u32];
        let mut interpolation = 0u32;
        for token in tokens.iter() {
            match token.kind() {
                TokenKind::FStringStart | TokenKind::TStringStart => interpolation += 1,
                TokenKind::FStringEnd | TokenKind::TStringEnd => {
                    interpolation = interpolation.saturating_sub(1)
                }
                _ if interpolation > 0 => {}
                TokenKind::Indent => {
                    indentation += 1;
                    columns.push(indentation_column(source, token.start()));
                }
                TokenKind::Dedent => {
                    if let Some((start, _, level)) = active {
                        if indentation == level {
                            ranges.push(TextRange::new(start, token.start()));
                            active = None;
                        }
                    }
                    indentation = indentation.saturating_sub(1);
                    columns.pop();
                }
                TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace => depth += 1,
                TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace => {
                    if let Some((start, level, _)) = active {
                        if depth == level {
                            ranges.push(TextRange::new(start, token.start()));
                            active = None;
                        }
                    }
                    depth = depth.saturating_sub(1);
                }
                TokenKind::Comment
                    if CommentLinePosition::for_range(token.range(), source).is_own_line() =>
                {
                    let controls =
                        super::super::comments::suppression::controls(&source[token.range()]);
                    if controls.contains(&SuppressionKind::On) {
                        if let Some((start, level, _)) = active.take() {
                            let closing = tokens
                                .in_range(TextRange::new(
                                    token.end(),
                                    TextSize::try_from(source.len()).unwrap(),
                                ))
                                .iter()
                                .find(|next| {
                                    !matches!(
                                        next.kind(),
                                        TokenKind::Newline
                                            | TokenKind::NonLogicalNewline
                                            | TokenKind::Indent
                                            | TokenKind::Dedent
                                    )
                                });
                            let end = closing
                                .filter(|next| {
                                    depth > level
                                        && matches!(
                                            next.kind(),
                                            TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace
                                        )
                                })
                                .map_or(token.start(), Ranged::end);
                            ranges.push(TextRange::new(start, end));
                        }
                    } else if controls.contains(&SuppressionKind::Off) && active.is_none() {
                        let column = indentation_column(source, token.start());
                        let next = tokens
                            .in_range(TextRange::new(
                                token.end(),
                                TextSize::try_from(source.len()).unwrap(),
                            ))
                            .iter()
                            .find(|next| {
                                !matches!(
                                    next.kind(),
                                    TokenKind::Newline
                                        | TokenKind::NonLogicalNewline
                                        | TokenKind::Comment
                                )
                            });
                        let level = columns
                            .iter()
                            .rposition(|known| *known == column)
                            .map(|index| u32::try_from(index).unwrap())
                            .unwrap_or_else(|| {
                                if next.is_some_and(|next| {
                                    next.kind() == TokenKind::Indent
                                        && indentation_column(source, next.start()) == column
                                }) {
                                    indentation + 1
                                } else {
                                    indentation
                                }
                            });
                        active = Some((token.end(), depth, level));
                    }
                }
                _ => {}
            }
        }
        if let Some((start, _, _)) = active {
            ranges.push(TextRange::new(
                start,
                TextSize::try_from(source.len()).unwrap(),
            ));
        }
        Self(ranges)
    }

    pub(crate) fn contains(&self, range: TextRange) -> bool {
        let index = self
            .0
            .partition_point(|region| region.end() <= range.start());
        self.0
            .get(index)
            .is_some_and(|region| region.contains_range(range))
    }

    pub(crate) fn resumes_in_decorators(&self, node: AnyNodeRef) -> bool {
        let (decorators, header) = match node {
            AnyNodeRef::StmtFunctionDef(function) => {
                (&function.decorator_list, function.name.start())
            }
            AnyNodeRef::StmtClassDef(class) => (&class.decorator_list, class.name.start()),
            _ => return false,
        };
        let Some(first) = decorators.first() else {
            return false;
        };
        self.0.iter().any(|region| {
            region.start() < header && region.end() > first.start() && region.end() <= header
        })
    }
}

fn indentation_column(source: &str, position: TextSize) -> u32 {
    let offset = usize::from(position);
    let start = source[..offset]
        .rfind('\n')
        .map_or(0, |newline| newline + 1);
    source[start..]
        .bytes()
        .take_while(|byte| matches!(byte, b' ' | b'\t' | b'\x0c'))
        .fold(0, |column, byte| match byte {
            b'\t' => column + 8 - column % 8,
            b'\x0c' => 0,
            _ => column + 1,
        })
}

pub(crate) fn format_fields(node: AnyNodeRef, f: &mut PyFormatter) -> FormatResult<bool> {
    if !(node.is_decorator()
        || node.is_expression()
        || node.is_keyword()
        || matches!(
            node,
            AnyNodeRef::StringLiteral(_)
                | AnyNodeRef::BytesLiteral(_)
                | AnyNodeRef::FString(_)
                | AnyNodeRef::TString(_)
        ))
        || !f.context().off_ranges().contains(node.range())
    {
        return Ok(false);
    }
    super::line::mark(node, node.range(), f.context().comments());
    write_range(node.range(), f)?;
    Ok(true)
}

pub(crate) fn write_range(range: TextRange, f: &mut PyFormatter) -> FormatResult<()> {
    let preceding = &f.context().source()[..usize::from(range.start())];
    let indentation = preceding
        .rsplit_once('\n')
        .map_or(preceding, |(_, prefix)| prefix);
    if indentation.bytes().all(|byte| matches!(byte, b' ' | b'\t')) {
        dedent_to_root(&format_args![
            hard_line_break(),
            text(indentation),
            crate::verbatim::verbatim_text(range)
        ])
        .fmt(f)
    } else {
        crate::verbatim::verbatim_text(range).fmt(f)
    }
}
