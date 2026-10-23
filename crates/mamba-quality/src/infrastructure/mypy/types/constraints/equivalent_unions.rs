//! Equivalent callable alternatives retain their common inference constraints.
use super::*;

impl Context {
    pub(super) fn equivalent_callable_constraints(
        &self,
        candidates: &[Type],
        actual: Type,
        variance: Variance,
        contextual: bool,
        bounds: &mut BTreeMap<Type, Bounds>,
    ) {
        if !candidates.iter().all(|&candidate| {
            self.node(candidate)
                .is_some_and(|node| matches!(node.as_ref(), Node::Callable(_)))
        }) {
            return;
        }
        let options: Vec<_> = candidates
            .iter()
            .map(|&candidate| {
                let mut option = BTreeMap::new();
                self.constraints(candidate, actual, variance, contextual, &mut option);
                for bound in option.values_mut() {
                    for values in [&mut bound.lower, &mut bound.upper, &mut bound.exact] {
                        values.sort_unstable();
                        values.dedup();
                    }
                }
                option
            })
            .collect();
        let Some(first) = options.first() else {
            return;
        };
        if !options.iter().all(|option| option == first) {
            return;
        }
        for (&variable, bound) in first {
            let target = bounds.entry(variable).or_default();
            target.lower.extend(&bound.lower);
            target.upper.extend(&bound.upper);
            target.exact.extend(&bound.exact);
        }
    }
}
