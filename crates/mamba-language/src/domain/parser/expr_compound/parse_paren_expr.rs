use super::*;

/// Parenthesized expr, tuple, or generator expression.
impl<'a> Parser<'a> {
    pub(crate) fn parse_paren_expr(&mut self) -> crate::error::Result<Spanned<Expr>> {
        let start = self.peek().unwrap().start;
        self.advance(); // consume (

        if self.peek_kind() == Some(TokenKind::RParen) {
            self.advance();
            return Ok(Spanned::new(Expr::TupleLit(vec![]), self.span_from(start)));
        }

        let first = self.parse_expr()?;

        // Generator expression: (expr for ...)
        if self.at_comprehension_clause_start() {
            let generators = self.parse_comprehension_clauses()?;
            self.expect(TokenKind::RParen)?;
            self.validate_comprehension_assignment_exprs(
                &[&first],
                &generators,
                self.span_from(start),
            )?;
            return Ok(Spanned::new(
                Expr::GeneratorExpr {
                    element: Box::new(first),
                    generators,
                },
                self.span_from(start),
            ));
        }

        // Tuple: (expr, ...)
        if self.peek_kind() == Some(TokenKind::Comma) {
            let mut elems = vec![first];
            while self.peek_kind() == Some(TokenKind::Comma) {
                self.advance();
                if self.peek_kind() == Some(TokenKind::RParen) {
                    break;
                }
                elems.push(self.parse_expr()?);
            }
            self.expect(TokenKind::RParen)?;
            return Ok(Spanned::new(Expr::TupleLit(elems), self.span_from(start)));
        }

        self.expect(TokenKind::RParen)?;
        Ok(first)
    }
}
