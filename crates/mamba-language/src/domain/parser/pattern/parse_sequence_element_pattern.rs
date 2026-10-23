use super::*;

/// Parse a single element inside a sequence pattern, allowing `*name` stars.
/// Star patterns are only valid in this context, not at the top-level (#827).
impl<'a> Parser<'a> {
    pub(super) fn parse_sequence_element_pattern(
        &mut self,
    ) -> crate::error::Result<Spanned<Pattern>> {
        let token = self
            .peek()
            .ok_or_else(|| MambaError::syntax(Span::dummy(), "expected pattern"))?;
        let start = token.start;
        if token.kind == TokenKind::Star {
            self.advance();
            let name = if self.peek_kind() == Some(TokenKind::Ident) {
                let text = self.current_text().to_string();
                self.advance();
                if text == "_" {
                    None
                } else {
                    Some(text)
                }
            } else {
                None
            };
            return Ok(Spanned::new(Pattern::Star(name), self.span_from(start)));
        }
        self.parse_pattern()
    }
}
