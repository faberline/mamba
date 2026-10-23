use super::*;

/// Parse `(a, b, *rest)` tuple-style sequence pattern.
/// - `()` → empty Sequence
/// - `(pat,)` → Sequence with one element
/// - `(pat, pat, ...)` → Sequence
/// - `(pat)` with no trailing comma → grouping (returns inner pattern, not Sequence)
impl<'a> Parser<'a> {
    pub(super) fn parse_paren_sequence_pattern(
        &mut self,
    ) -> crate::error::Result<Spanned<Pattern>> {
        let start = self.peek().unwrap().start;
        self.advance(); // consume (

        // Empty parens: ()
        if self.peek_kind() == Some(TokenKind::RParen) {
            self.advance();
            return Ok(Spanned::new(
                Pattern::Sequence(vec![]),
                self.span_from(start),
            ));
        }

        let first = self.parse_sequence_element_pattern()?;

        // If next is `)` without a trailing comma: grouping, return inner pattern
        if self.peek_kind() == Some(TokenKind::RParen) {
            self.advance();
            return Ok(first);
        }

        // Trailing comma or more elements → this is a Sequence
        self.expect(TokenKind::Comma)?;
        let mut patterns = vec![first];

        while self.peek_kind() != Some(TokenKind::RParen)
            && self.peek_kind() != Some(TokenKind::Eof)
        {
            patterns.push(self.parse_sequence_element_pattern()?);
            if self.peek_kind() != Some(TokenKind::RParen) {
                self.expect(TokenKind::Comma)?;
            }
        }
        self.expect(TokenKind::RParen)?;
        Ok(Spanned::new(
            Pattern::Sequence(patterns),
            self.span_from(start),
        ))
    }
}
