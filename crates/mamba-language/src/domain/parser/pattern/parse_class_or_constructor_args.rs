use super::*;

/// Parse `(...)` args for constructor or class pattern.
/// Distinguishes between:
/// - Constructor: `Circle(r, g)` — all positional idents
/// - ClassPattern: `Point(x=1, y=2)` — has `name=pattern` form
impl<'a> Parser<'a> {
    pub(super) fn parse_class_or_constructor_args(
        &mut self,
        cls: Vec<Name>,
        start: u32,
    ) -> crate::error::Result<Spanned<Pattern>> {
        self.advance(); // consume (

        // Empty parens
        if self.peek_kind() == Some(TokenKind::RParen) {
            self.advance();
            return Ok(Spanned::new(
                Pattern::ClassPattern {
                    cls,
                    patterns: Vec::new(),
                },
                self.span_from(start),
            ));
        }

        // Always parse as ClassPattern to support mixed positional/keyword args (PEP 634 #827)
        let mut patterns = Vec::new();
        let mut seen_keyword = false;
        while self.peek_kind() != Some(TokenKind::RParen)
            && self.peek_kind() != Some(TokenKind::Eof)
        {
            // PEP 634: star patterns are not allowed inside class patterns.
            if self.peek_kind() == Some(TokenKind::Star) {
                let span = self
                    .peek()
                    .map(|t| Span::new(self.file_id, t.start, t.end))
                    .unwrap_or_else(Span::dummy);
                return Err(MambaError::syntax(
                    span,
                    "starred patterns are not allowed in class patterns",
                ));
            }
            if self.peek_kind() == Some(TokenKind::Ident) {
                let saved = self.pos;
                let (ns, ne) = self.expect(TokenKind::Ident)?;
                let name_text = self.text_at(ns, ne).to_string();
                if self.peek_kind() == Some(TokenKind::Eq) {
                    self.advance(); // consume =
                    let pat = self.parse_pattern()?;
                    patterns.push((Some(name_text), pat));
                    seen_keyword = true;
                } else {
                    // Backtrack — positional pattern starting with an ident
                    // PEP 634: positional may not follow keyword.
                    if seen_keyword {
                        let span = self
                            .peek()
                            .map(|t| Span::new(self.file_id, t.start, t.end))
                            .unwrap_or_else(Span::dummy);
                        return Err(MambaError::syntax(
                            span,
                            "positional pattern argument may not follow keyword argument",
                        ));
                    }
                    self.pos = saved;
                    let pat = self.parse_pattern()?;
                    patterns.push((None, pat));
                }
            } else {
                // Positional pattern (literal, sequence, mapping, etc.)
                // PEP 634: positional may not follow keyword.
                if seen_keyword {
                    let span = self
                        .peek()
                        .map(|t| Span::new(self.file_id, t.start, t.end))
                        .unwrap_or_else(Span::dummy);
                    return Err(MambaError::syntax(
                        span,
                        "positional pattern argument may not follow keyword argument",
                    ));
                }
                let pat = self.parse_pattern()?;
                patterns.push((None, pat));
            }
            if self.peek_kind() != Some(TokenKind::RParen) {
                self.expect(TokenKind::Comma)?;
            }
        }
        self.expect(TokenKind::RParen)?;
        Ok(Spanned::new(
            Pattern::ClassPattern { cls, patterns },
            self.span_from(start),
        ))
    }
}
