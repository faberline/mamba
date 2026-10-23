use super::*;

impl<'a> Parser<'a> {
    pub(super) fn validate_pep695_fstring_part(
        &self,
        part: &FStringPart,
        context: Pep695ExprContext,
    ) -> crate::error::Result<()> {
        if let FStringPart::Expr(value, format) = part {
            self.validate_pep695_expr(value, context)?;
            if let Some(format) = format {
                for part in format {
                    self.validate_pep695_fstring_part(part, context)?;
                }
            }
        }
        Ok(())
    }
}
