use super::*;

/// Parse a single pattern (no OR at this level).
impl<'a> Parser<'a> {
    pub(super) fn parse_single_pattern(&mut self) -> crate::error::Result<Spanned<Pattern>> {
        let token = self
            .peek()
            .ok_or_else(|| MambaError::syntax(Span::dummy(), "expected pattern"))?;
        let start = token.start;

        match &token.kind {
            // Star patterns (`*rest`) are only valid inside sequence patterns.
            // Reject them at the top-level single-pattern parse so that invalid
            // syntax like `case *rest:` is caught with a clear error (#827).
            TokenKind::Star => Err(MambaError::syntax(
                Span::new(self.file_id, start, token.end),
                "star pattern '*name' is only valid inside a sequence pattern like [*rest, x]"
                    .to_string(),
            )),
            // Sequence pattern: `[a, b, *rest]`
            TokenKind::LBracket => self.parse_sequence_pattern(),
            // Tuple sequence pattern: `(a, b)` — PEP 634 treats these as sequence patterns
            TokenKind::LParen => self.parse_paren_sequence_pattern(),
            // Mapping pattern: `{"key": value}`
            TokenKind::LBrace => self.parse_mapping_pattern(),
            // Wildcard: `_`
            TokenKind::Ident if self.current_text() == "_" => {
                self.advance();
                Ok(Spanned::new(Pattern::Wildcard, self.span_from(start)))
            }
            // Ident: binding, constructor, or class pattern
            TokenKind::Ident => self.parse_ident_pattern(),
            // Built-in type keywords as class pattern heads: `int(n)`, `str(s)`, etc.
            TokenKind::IntType
            | TokenKind::FloatType
            | TokenKind::BoolType
            | TokenKind::StrType
            | TokenKind::ListType
            | TokenKind::DictType
            | TokenKind::TupleType => self.parse_builtin_type_pattern(),
            // Literal patterns
            TokenKind::Int(_)
            | TokenKind::Float(_)
            | TokenKind::Str(_)
            | TokenKind::True
            | TokenKind::False
            | TokenKind::None_ => {
                let expr = self.parse_pattern_literal()?;
                Ok(Spanned::new(Pattern::Literal(expr.node), expr.span))
            }
            // Negative number literals: `-42`
            TokenKind::Minus => {
                let expr = self.parse_pattern_literal()?;
                Ok(Spanned::new(Pattern::Literal(expr.node), expr.span))
            }
            _ => Err(MambaError::syntax(
                Span::new(self.file_id, start, token.end),
                format!("expected pattern, got {}", token.kind),
            )),
        }
    }
}
