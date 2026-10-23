use super::*;

impl<'a> Parser<'a> {
    pub(super) fn parse_call(
        &mut self,
        func: Spanned<Expr>,
    ) -> crate::error::Result<Spanned<Expr>> {
        self.expect(TokenKind::LParen)?;
        let mut args = Vec::new();
        while self.peek_kind() != Some(TokenKind::RParen)
            && self.peek_kind() != Some(TokenKind::Eof)
        {
            if self.peek_kind() == Some(TokenKind::DoubleStar) {
                self.advance();
                let expr = self.parse_expr()?;
                args.push(CallArg::DoubleStarArg(expr));
            } else if self.peek_kind() == Some(TokenKind::Star) {
                self.advance();
                let expr = self.parse_expr()?;
                args.push(CallArg::StarArg(expr));
            } else {
                let expr = self.parse_expr()?;
                // Generator expression as sole argument: `f(x for x in ...)`
                if self.at_comprehension_clause_start() {
                    let generators = self.parse_comprehension_clauses()?;
                    let span = expr.span;
                    let gen = Spanned::new(
                        Expr::GeneratorExpr {
                            element: Box::new(expr),
                            generators,
                        },
                        span,
                    );
                    args.push(CallArg::Positional(gen));
                    break; // generator expr is the only argument
                }
                // Check for keyword arg: `name=value`
                if self.peek_kind() == Some(TokenKind::Eq) {
                    if let Expr::Ident(name) = &expr.node {
                        let name = name.clone();
                        self.advance(); // consume =
                        let value = self.parse_expr()?;
                        args.push(CallArg::Keyword { name, value });
                    } else {
                        args.push(CallArg::Positional(expr));
                    }
                } else {
                    args.push(CallArg::Positional(expr));
                }
            }
            if self.peek_kind() != Some(TokenKind::RParen) {
                self.expect(TokenKind::Comma)?;
            }
        }
        let (end_s, end_e) = self.expect(TokenKind::RParen)?;
        let span = func.span.merge(Span::new(self.file_id, end_s, end_e));
        Ok(Spanned::new(
            Expr::Call {
                func: Box::new(func),
                args,
            },
            span,
        ))
    }
}
