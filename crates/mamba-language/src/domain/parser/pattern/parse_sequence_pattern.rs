use super::*;

/// Parse `[a, b, *rest]` sequence pattern.
impl<'a> Parser<'a> {
    pub(super) fn parse_sequence_pattern(&mut self) -> crate::error::Result<Spanned<Pattern>> {
        let start = self.peek().unwrap().start;
        self.advance(); // consume [

        let mut patterns = Vec::new();
        while self.peek_kind() != Some(TokenKind::RBracket)
            && self.peek_kind() != Some(TokenKind::Eof)
        {
            // Star elements are valid only inside sequences.
            patterns.push(self.parse_sequence_element_pattern()?);
            if self.peek_kind() != Some(TokenKind::RBracket) {
                self.expect(TokenKind::Comma)?;
            }
        }
        self.expect(TokenKind::RBracket)?;

        // PEP 634: at most one starred element is allowed in a sequence pattern.
        let star_count = patterns
            .iter()
            .filter(|p| matches!(&p.node, Pattern::Star(_)))
            .count();
        if star_count > 1 {
            return Err(MambaError::syntax(
                self.span_from(start),
                "sequence pattern may only have one starred element",
            ));
        }

        Ok(Spanned::new(
            Pattern::Sequence(patterns),
            self.span_from(start),
        ))
    }
}
