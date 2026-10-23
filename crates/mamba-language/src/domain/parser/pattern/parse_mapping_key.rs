use super::*;

/// Parse a mapping-pattern key. PEP 634 permits literal keys and dotted
/// value-pattern keys (`case {Keys.KEY: value}:`), but not bare capture
/// names or arbitrary expressions.
impl<'a> Parser<'a> {
    pub(super) fn parse_mapping_key(&mut self) -> crate::error::Result<Spanned<Expr>> {
        let token = self
            .peek()
            .ok_or_else(|| MambaError::syntax(Span::dummy(), "expected mapping pattern key"))?;
        match &token.kind {
            TokenKind::Int(_)
            | TokenKind::Float(_)
            | TokenKind::Str(_)
            | TokenKind::True
            | TokenKind::False
            | TokenKind::None_
            | TokenKind::Minus => self.parse_pattern_literal(),
            kind if Parser::is_name_token(kind) => self.parse_dotted_value_key(),
            other => Err(MambaError::syntax(
                Span::new(self.file_id, token.start, token.end),
                format!("expected literal or dotted value key, got {other}"),
            )),
        }
    }
}
