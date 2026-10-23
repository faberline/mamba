use super::*;

/// Parse a match pattern (with OR: `p1 | p2 | p3`, and AS: `p as name`).
impl<'a> Parser<'a> {
    pub fn parse_pattern(&mut self) -> crate::error::Result<Spanned<Pattern>> {
        let first = self.parse_single_pattern()?;
        let start = first.span.start;

        // OR pattern: `p1 | p2`
        let pat = if self.peek_kind() == Some(TokenKind::Pipe) {
            let mut alternatives = vec![first];
            while self.peek_kind() == Some(TokenKind::Pipe) {
                self.advance();
                alternatives.push(self.parse_single_pattern()?);
            }
            Spanned::new(Pattern::Or(alternatives), self.span_from(start))
        } else {
            first
        };

        // AS pattern: `p as name` (PEP 634, #827)
        if self.peek_kind() == Some(TokenKind::As) {
            self.advance(); // consume `as`
            let (ns, ne) = self.expect(TokenKind::Ident)?;
            let name = self.text_at(ns, ne).to_string();
            return Ok(Spanned::new(
                Pattern::As {
                    pattern: Box::new(pat),
                    name,
                },
                self.span_from(start),
            ));
        }

        Ok(pat)
    }
}
