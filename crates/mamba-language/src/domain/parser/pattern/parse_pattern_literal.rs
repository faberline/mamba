use super::*;

/// Parse a literal for use in a pattern: atom or negated atom only.
/// Does NOT call parse_expr to avoid consuming `|` as bitwise OR.
impl<'a> Parser<'a> {
    pub(super) fn parse_pattern_literal(&mut self) -> crate::error::Result<Spanned<Expr>> {
        let start = self.peek().unwrap().start;
        let negative = if self.peek_kind() == Some(TokenKind::Minus) {
            self.advance();
            true
        } else {
            false
        };
        // Parse just the atom (number, string, bool, None)
        let inner = self.parse_literal_atom()?;
        if negative {
            let span = self.span_from(start);
            Ok(Spanned::new(
                Expr::UnaryOp {
                    op: UnaryOp::Neg,
                    operand: Box::new(inner),
                },
                span,
            ))
        } else {
            Ok(inner)
        }
    }
}
