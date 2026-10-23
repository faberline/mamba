use super::*;

/// Parse an f-string expression through the full parser.
/// Falls back to `Ident(s)` if parsing fails (e.g., keyword args).
pub(super) fn parse_fstring_expr(s: &str, raw_lexer: super::super::RawLexer) -> Expr {
    let tokens = crate::domain::indent::IndentProcessor::new()
        .process(raw_lexer(s, crate::source::span::FileId(0)));
    let mut parser =
        super::super::Parser::new(tokens, s, crate::source::span::FileId(0), raw_lexer);
    match parser.parse_expr_bp(0) {
        Ok(spanned) => spanned.node,
        Err(_) => Expr::Ident(s.to_string()),
    }
}
