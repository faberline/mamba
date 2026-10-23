//! Trailing tuple commas follow logical lines and the original token predicate.
use super::Checker;
use mamba_language::application::quality_syntax::{python_ast::token::TokenKind, SyntaxDocument};
use ruff_text_size::Ranged;

impl Checker<'_> {
    pub(super) fn check_trailing_tuples(&mut self, document: &SyntaxDocument<'_>) {
        let mut starts = super::source_layout::lines(document.source)
            .into_iter()
            .map(|(start, _)| start)
            .collect::<Vec<_>>();
        if starts.is_empty() || document.source.ends_with(['\r', '\n']) {
            starts.push(document.source.len());
        }
        let line = |offset: usize| {
            starts
                .partition_point(|start| *start <= offset)
                .saturating_sub(1)
        };
        let tokens = document.parsed.tokens();
        let mut logical_start = 0;
        for (index, token) in tokens.iter().enumerate() {
            if token.kind() == TokenKind::Newline {
                logical_start = index + 1;
                continue;
            }
            if token.kind() != TokenKind::Comma {
                continue;
            }
            let current = line(token.start().to_usize());
            let following = tokens[index + 1..]
                .iter()
                .filter(|token| {
                    !matches!(
                        token.kind(),
                        TokenKind::Indent | TokenKind::Dedent | TokenKind::EndOfFile
                    )
                })
                .take_while(|token| line(token.start().to_usize()) == current)
                .collect::<Vec<_>>();
            if following.is_empty()
                || following
                    .iter()
                    .any(|token| !matches!(token.kind(), TokenKind::Newline | TokenKind::Comment))
            {
                continue;
            }
            if tokens[logical_start..index].iter().any(|token| {
                matches!(token.kind(), TokenKind::Return | TokenKind::Yield)
                    || document.source[token.range()].contains('=')
            }) {
                self.layout_diagnostic(
                    "R1707",
                    "trailing-comma-tuple",
                    "Disallow trailing comma tuple",
                    current as u32 + 1,
                    0,
                    None,
                    token.start().to_usize(),
                );
            }
        }
    }
}
