use super::*;

impl<'a> Parser<'a> {
    pub(crate) fn validate_pep695_generic_exprs(
        &self,
        exprs: &[Spanned<Expr>],
    ) -> crate::error::Result<()> {
        for expr in exprs {
            self.validate_pep695_expr(expr, Pep695ExprContext::GenericDefinition)?;
        }
        Ok(())
    }
}
