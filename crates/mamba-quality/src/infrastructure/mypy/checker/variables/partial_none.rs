use super::*;

impl Checker<'_> {
    pub(super) fn resolve_literal_tuple_partial_none(&mut self, target: &Expr, value: &Expr) {
        if !matches!(target, Expr::Tuple(_) | Expr::List(_)) {
            return;
        }
        let mut leaves = Vec::new();
        if !matching_literal_leaves(target, value, &mut leaves) {
            return;
        }
        let inferred_leaves = leaves
            .into_iter()
            .filter(|(target, _)| target.is_name_expr())
            .map(|(target, value)| (target, self.infer(value)))
            .collect::<Vec<_>>();
        for (target, actual) in inferred_leaves {
            if self.scopes.resolve_partial_none(target, actual).is_some() {
                self.refuse_local_none_inputs();
            }
        }
    }
}

fn matching_literal_leaves<'a>(
    target: &'a Expr,
    value: &'a Expr,
    leaves: &mut Vec<(&'a Expr, &'a Expr)>,
) -> bool {
    let target_items: Option<&[Expr]> = match target {
        Expr::Tuple(tuple) => Some(tuple.elts.as_ref()),
        Expr::List(list) => Some(list.elts.as_ref()),
        _ => None,
    };
    if let Some(target_items) = target_items {
        let value_items: &[Expr] = match value {
            Expr::Tuple(tuple) => tuple.elts.as_ref(),
            Expr::List(list) => list.elts.as_ref(),
            _ => return false,
        };
        if target_items.len() != value_items.len()
            || target_items.iter().any(Expr::is_starred_expr)
            || value_items.iter().any(Expr::is_starred_expr)
        {
            return false;
        }
        let start = leaves.len();
        for (target, value) in target_items.iter().zip(value_items) {
            if !matching_literal_leaves(target, value, leaves) {
                leaves.truncate(start);
                return false;
            }
        }
        true
    } else if target.is_starred_expr() || value.is_starred_expr() {
        false
    } else {
        leaves.push((target, value));
        true
    }
}
