use super::*;

/// Ternary: `body if condition else else_body`
impl<'a> Parser<'a> {
    pub(crate) fn parse_ternary(
        &mut self,
        body: Spanned<Expr>,
    ) -> crate::error::Result<Spanned<Expr>> {
        let start = body.span.start;
        self.expect(TokenKind::If)?;
        let condition = self.parse_expr_bp(0)?;
        self.expect(TokenKind::Else)?;
        let else_body = self.parse_expr()?;
        let span = Span::new(self.file_id, start, else_body.span.end);
        Ok(Spanned::new(
            Expr::IfExpr {
                body: Box::new(body),
                condition: Box::new(condition),
                else_body: Box::new(else_body),
            },
            span,
        ))
    }
}
