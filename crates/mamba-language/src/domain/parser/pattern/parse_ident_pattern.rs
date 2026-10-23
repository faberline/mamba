use super::*;

/// Parse ident-starting patterns: bindings, constructors, or class patterns.
impl<'a> Parser<'a> {
    pub(super) fn parse_ident_pattern(&mut self) -> crate::error::Result<Spanned<Pattern>> {
        let start = self.peek().unwrap().start;
        let first = self.current_text().to_string();
        self.advance();

        // Dotted path: `Shape.Circle(r)` or `mod.Class(x=1)`
        if self.peek_kind() == Some(TokenKind::Dot) {
            let mut path = vec![first];
            while self.peek_kind() == Some(TokenKind::Dot) {
                self.advance();
                let (ps, pe) = self.expect(TokenKind::Ident)?;
                path.push(self.text_at(ps, pe).to_string());
            }
            if self.peek_kind() == Some(TokenKind::LParen) {
                return self.parse_class_or_constructor_args(path, start);
            }
            // Dotted name without parens — treat as constructor with no fields
            return Ok(Spanned::new(
                Pattern::Constructor {
                    path,
                    fields: Vec::new(),
                },
                self.span_from(start),
            ));
        }

        // Parens after bare ident: `Circle(r)` or `ClassName(x=1)`
        if self.peek_kind() == Some(TokenKind::LParen) {
            return self.parse_class_or_constructor_args(vec![first], start);
        }

        // Simple binding
        Ok(Spanned::new(Pattern::Binding(first), self.span_from(start)))
    }
}
