use super::*;

/// List literal or list comprehension: `[...]`
impl<'a> Parser<'a> {
    pub(crate) fn parse_list_or_comp(&mut self) -> crate::error::Result<Spanned<Expr>> {
        let start = self.peek().unwrap().start;
        self.advance(); // consume [

        if self.peek_kind() == Some(TokenKind::RBracket) {
            self.advance();
            return Ok(Spanned::new(Expr::ListLit(vec![]), self.span_from(start)));
        }

        let first = self.parse_expr()?;

        // List comprehension: [expr for ...]
        if self.at_comprehension_clause_start() {
            let generators = self.parse_comprehension_clauses()?;
            self.expect(TokenKind::RBracket)?;
            self.validate_comprehension_assignment_exprs(
                &[&first],
                &generators,
                self.span_from(start),
            )?;
            return Ok(Spanned::new(
                Expr::ListComp {
                    element: Box::new(first),
                    generators,
                },
                self.span_from(start),
            ));
        }

        // Regular list
        let mut elems = vec![first];
        while self.peek_kind() == Some(TokenKind::Comma) {
            self.advance();
            if self.peek_kind() == Some(TokenKind::RBracket) {
                break;
            }
            elems.push(self.parse_expr()?);
        }
        self.expect(TokenKind::RBracket)?;
        Ok(Spanned::new(Expr::ListLit(elems), self.span_from(start)))
    }
}
