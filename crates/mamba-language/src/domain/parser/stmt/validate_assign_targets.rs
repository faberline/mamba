use super::*;

impl<'a> Parser<'a> {
    pub(super) fn validate_assign_targets(
        targets: &[Spanned<Expr>],
        message: &str,
    ) -> crate::error::Result<()> {
        for target in targets {
            Self::validate_assign_target(target, message)?;
        }
        Ok(())
    }
}
