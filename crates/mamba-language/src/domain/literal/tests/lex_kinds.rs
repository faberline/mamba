use super::*;

/// Helper: lex source and collect token kinds (skipping errors).
pub(super) fn lex_kinds(source: &str) -> Vec<TokenKind> {
    crate::lexer::lex_raw(source, crate::source::span::FileId(0))
        .into_iter()
        .map(|token| token.kind)
        .collect()
}
