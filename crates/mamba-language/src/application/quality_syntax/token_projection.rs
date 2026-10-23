//! Rich token views share one native-token range walk.
use super::SyntaxToken;
use ruff_python_ast::token::Tokens;
use ruff_text_size::Ranged;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Projection {
    Full,
    Comments,
}

pub(super) fn project(source: &str, native: &Tokens, projection: Projection) -> Vec<SyntaxToken> {
    let full = projection == Projection::Full;
    let mut tokens = Vec::new();
    let mut cursor = 0;
    for token in native.iter() {
        let range = token.range();
        let start = range.start().to_u32();
        let end = range.end().to_u32();
        if start > cursor {
            if full {
                let text = &source[cursor as usize..start as usize];
                let trivia = text
                    .chars()
                    .all(|character| character.is_whitespace() || character == '\\');
                tokens.push(SyntaxToken {
                    kind: if trivia { "Whitespace" } else { "Unlexed" }.into(),
                    start: cursor,
                    end: start,
                    trivia,
                });
            }
            cursor = start;
        }
        if end > start && start >= cursor {
            let kind = token.kind();
            if full || kind.is_comment() {
                tokens.push(SyntaxToken {
                    kind: format!("{kind:?}"),
                    start,
                    end,
                    trivia: kind.is_comment() || kind.is_any_newline(),
                });
            }
            cursor = end;
        }
    }
    if full && (cursor as usize) < source.len() {
        tokens.push(SyntaxToken {
            kind: "TrailingTrivia".into(),
            start: cursor,
            end: source.len() as u32,
            trivia: source[cursor as usize..].trim().is_empty(),
        });
    }
    tokens
}
