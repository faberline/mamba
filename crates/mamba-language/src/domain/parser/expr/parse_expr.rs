use super::*;

/// Parse an expression (top-level with yield/lambda/await/ternary/walrus).
impl<'a> Parser<'a> {
    pub fn parse_expr(&mut self) -> crate::error::Result<Spanned<Expr>> {
        // A walrus at the top of an *expression statement* is illegal in
        // CPython (`a := 5` must be parenthesized). Capture-and-clear the flag
        // so only this level — not parenthesized/nested sub-expressions, which
        // recurse through parse_expr with the flag already false — is treated
        // as statement-top.
        let stmt_top = self.stmt_expr_toplevel;
        self.stmt_expr_toplevel = false;
        if self.peek_kind() == Some(TokenKind::Yield) {
            return self.parse_yield_expr();
        }
        if self.peek_kind() == Some(TokenKind::Lambda) {
            return self.parse_lambda();
        }
        if self.peek_kind() == Some(TokenKind::Star) {
            let start = self.peek().unwrap().start;
            self.advance();
            let operand = self.parse_expr()?;
            let span = Span::new(self.file_id, start, operand.span.end);
            return Ok(Spanned::new(Expr::Starred(Box::new(operand)), span));
        }

        let expr = self.parse_expr_bp(0)?;

        // Ternary: `expr if cond else alt`
        if self.peek_kind() == Some(TokenKind::If) {
            return self.parse_ternary(expr);
        }
        // Walrus: `name := expr`
        if self.peek_kind() == Some(TokenKind::ColonEq) {
            if let Expr::Ident(ref name) = expr.node {
                // A bare walrus as an expression statement (`a := 5`) is a
                // SyntaxError; it must be parenthesized (`(a := 5)`). Reaching
                // this branch with `stmt_top` set means the `:=` is at the
                // statement's top level (a parenthesized walrus is consumed by
                // a deeper parse_expr call, with the flag already cleared).
                if stmt_top {
                    return Err(crate::error::MambaError::syntax(
                        expr.span,
                        "invalid syntax",
                    ));
                }
                let name = name.clone();
                let start = expr.span.start;
                self.advance();
                let value = self.parse_expr()?;
                let span = Span::new(self.file_id, start, value.span.end);
                return Ok(Spanned::new(
                    Expr::Walrus {
                        target: name,
                        value: Box::new(value),
                    },
                    span,
                ));
            }
        }
        Ok(expr)
    }
}
