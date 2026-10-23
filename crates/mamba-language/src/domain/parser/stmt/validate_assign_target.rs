use super::*;

impl<'a> Parser<'a> {
    pub(super) fn validate_assign_target(
        target: &Spanned<Expr>,
        message: &str,
    ) -> crate::error::Result<()> {
        match &target.node {
            Expr::Ident(_) | Expr::Attr { .. } | Expr::Index { .. } => Ok(()),
            Expr::TupleLit(elems) | Expr::ListLit(elems) | Expr::UnpackTarget(elems) => {
                for elem in elems {
                    Self::validate_assign_target(elem, message)?;
                }
                Ok(())
            }
            Expr::Starred(inner) => Self::validate_assign_target(inner, message),
            _ => Err(MambaError::syntax(target.span, message.to_string())),
        }
    }
}
