use super::*;

/// Build `path[0].path[1]....attr = (P1, P2, ...)`.
pub(super) fn attr_tuple_assign(
    path: &[Name],
    attr: &str,
    binding_names: &[Name],
    span: Span,
) -> Spanned<Stmt> {
    attr_assign(path, attr, params_tuple(binding_names, span), span)
}
