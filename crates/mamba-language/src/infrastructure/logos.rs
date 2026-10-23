//! Logos adapter. It moves literal payloads into shared token data.
mod conversion;
mod kind;
mod numbers;
mod strings;

use kind::LogosKind;
use logos::Logos;
use mamba_shared_kernel::source::span::FileId;
use mamba_shared_kernel::token::Token;

pub(crate) fn lex_raw(source: &str, _file_id: FileId) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut lexer = LogosKind::lexer(source);
    while let Some(result) = lexer.next() {
        let span = lexer.span();
        if let Ok(kind) = result {
            tokens.push(Token::new(kind.into(), span.start as u32, span.end as u32));
        }
    }
    tokens
}
