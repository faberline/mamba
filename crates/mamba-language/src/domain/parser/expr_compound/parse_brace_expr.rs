use super::*;

/// Dict/Set literal or comprehension: `{...}`
impl<'a> Parser<'a> {
    pub(crate) fn parse_brace_expr(&mut self) -> crate::error::Result<Spanned<Expr>> {
        let start = self.peek().unwrap().start;
        self.advance(); // consume {

        // Empty dict
        if self.peek_kind() == Some(TokenKind::RBrace) {
            self.advance();
            return Ok(Spanned::new(Expr::DictLit(vec![]), self.span_from(start)));
        }

        // Dict unpack entry `{**expr, ...}` (PEP 448 / #1014)
        if self.peek_kind() == Some(TokenKind::DoubleStar) {
            self.advance();
            let val = self.parse_expr()?;
            let mut entries: Vec<(Option<Spanned<Expr>>, Spanned<Expr>)> = vec![(None, val)];
            while self.peek_kind() == Some(TokenKind::Comma) {
                self.advance();
                if self.peek_kind() == Some(TokenKind::RBrace) {
                    break;
                }
                if self.peek_kind() == Some(TokenKind::DoubleStar) {
                    self.advance();
                    let v = self.parse_expr()?;
                    entries.push((None, v));
                } else {
                    let k = self.parse_expr()?;
                    self.expect(TokenKind::Colon)?;
                    let v = self.parse_expr()?;
                    entries.push((Some(k), v));
                }
            }
            self.expect(TokenKind::RBrace)?;
            return Ok(Spanned::new(Expr::DictLit(entries), self.span_from(start)));
        }

        let first = self.parse_expr()?;

        // Dict or dict comprehension: {key: value ...}
        if self.peek_kind() == Some(TokenKind::Colon) {
            self.advance();
            let value = self.parse_expr()?;

            // Dict comprehension: {k: v for ...}
            if self.at_comprehension_clause_start() {
                let generators = self.parse_comprehension_clauses()?;
                self.expect(TokenKind::RBrace)?;
                self.validate_comprehension_assignment_exprs(
                    &[&first, &value],
                    &generators,
                    self.span_from(start),
                )?;
                return Ok(Spanned::new(
                    Expr::DictComp {
                        key: Box::new(first),
                        value: Box::new(value),
                        generators,
                    },
                    self.span_from(start),
                ));
            }

            // Regular dict (possibly mixed with **unpack entries)
            let mut pairs: Vec<(Option<Spanned<Expr>>, Spanned<Expr>)> = vec![(Some(first), value)];
            while self.peek_kind() == Some(TokenKind::Comma) {
                self.advance();
                if self.peek_kind() == Some(TokenKind::RBrace) {
                    break;
                }
                if self.peek_kind() == Some(TokenKind::DoubleStar) {
                    self.advance();
                    let v = self.parse_expr()?;
                    pairs.push((None, v));
                } else {
                    let k = self.parse_expr()?;
                    self.expect(TokenKind::Colon)?;
                    let v = self.parse_expr()?;
                    pairs.push((Some(k), v));
                }
            }
            self.expect(TokenKind::RBrace)?;
            return Ok(Spanned::new(Expr::DictLit(pairs), self.span_from(start)));
        }

        // Set comprehension: {expr for ...}
        if self.at_comprehension_clause_start() {
            let generators = self.parse_comprehension_clauses()?;
            self.expect(TokenKind::RBrace)?;
            self.validate_comprehension_assignment_exprs(
                &[&first],
                &generators,
                self.span_from(start),
            )?;
            return Ok(Spanned::new(
                Expr::SetComp {
                    element: Box::new(first),
                    generators,
                },
                self.span_from(start),
            ));
        }

        // Set literal: {expr, ...}
        let mut elems = vec![first];
        while self.peek_kind() == Some(TokenKind::Comma) {
            self.advance();
            if self.peek_kind() == Some(TokenKind::RBrace) {
                break;
            }
            elems.push(self.parse_expr()?);
        }
        self.expect(TokenKind::RBrace)?;
        Ok(Spanned::new(Expr::SetLit(elems), self.span_from(start)))
    }
}
