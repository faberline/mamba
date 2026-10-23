use super::*;

/// Await: `await expr`
impl<'a> Parser<'a> {
    pub(crate) fn parse_await_expr(&mut self) -> crate::error::Result<Spanned<Expr>> {
        let (start, _) = self.advance(); // consume `await`
        let bp = super::super::expr::prefix_bp(&UnaryOp::Neg);
        let expr = self.parse_expr_bp(bp)?;
        let span = Span::new(self.file_id, start, expr.span.end);
        Ok(Spanned::new(Expr::Await(Box::new(expr)), span))
    }
}
