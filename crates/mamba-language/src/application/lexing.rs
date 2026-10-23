use mamba_shared_kernel::source::span::{FileId, Span};
use mamba_shared_kernel::token::Token;

pub fn lex(source: &str, file_id: FileId) -> Vec<Token> {
    crate::domain::indent::IndentProcessor::new().process(lex_raw(source, file_id))
}

pub fn lex_raw(source: &str, file_id: FileId) -> Vec<Token> {
    (crate::app::RAW_LEXER)(source, file_id)
}

pub fn token_span(token: &Token, file_id: FileId) -> Span {
    Span::new(file_id, token.start, token.end)
}

#[cfg(test)]
mod tests_files;
