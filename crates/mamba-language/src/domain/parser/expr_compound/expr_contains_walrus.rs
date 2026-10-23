use super::*;

impl<'a> Parser<'a> {
    pub(super) fn expr_contains_walrus(expr: &Spanned<Expr>) -> bool {
        let mut targets = Vec::new();
        Self::collect_walrus_targets(expr, &mut targets);
        !targets.is_empty()
    }
}
