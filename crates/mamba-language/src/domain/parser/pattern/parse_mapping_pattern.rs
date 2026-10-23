use super::*;

/// Parse `{"key": value, ..., **rest}` mapping pattern (#827).
impl<'a> Parser<'a> {
    pub(super) fn parse_mapping_pattern(&mut self) -> crate::error::Result<Spanned<Pattern>> {
        let start = self.peek().unwrap().start;
        self.advance(); // consume {

        let mut pairs = Vec::new();
        let mut rest: Option<Name> = None;

        while self.peek_kind() != Some(TokenKind::RBrace)
            && self.peek_kind() != Some(TokenKind::Eof)
        {
            // Rest capture: `**rest`
            // PEP 634: `**_` is not valid syntax; a real identifier is required.
            if self.peek_kind() == Some(TokenKind::DoubleStar) {
                self.advance(); // consume **
                let name = if self.peek_kind() == Some(TokenKind::Ident) {
                    let text = self.current_text().to_string();
                    if text == "_" {
                        let span = self
                            .peek()
                            .map(|t| Span::new(self.file_id, t.start, t.end))
                            .unwrap_or_else(Span::dummy);
                        return Err(MambaError::syntax(
                            span,
                            "mapping rest pattern requires an identifier, \
                             `**_` is not valid; use `case {**rest}:` to capture remaining entries",
                        ));
                    }
                    self.advance();
                    Some(text)
                } else {
                    let span = self
                        .peek()
                        .map(|t| Span::new(self.file_id, t.start, t.end))
                        .unwrap_or_else(Span::dummy);
                    return Err(MambaError::syntax(
                        span,
                        "expected identifier after `**` in mapping pattern",
                    ));
                };
                rest = name;
                // `**rest` must be last; skip optional trailing comma
                if self.peek_kind() == Some(TokenKind::Comma) {
                    self.advance();
                }
                break;
            }
            let key = self.parse_mapping_key()?;
            self.expect(TokenKind::Colon)?;
            let value = self.parse_pattern()?;
            pairs.push((key, value));
            if self.peek_kind() != Some(TokenKind::RBrace) {
                self.expect(TokenKind::Comma)?;
            }
        }
        self.expect(TokenKind::RBrace)?;
        Ok(Spanned::new(
            Pattern::Mapping { pairs, rest },
            self.span_from(start),
        ))
    }
}
