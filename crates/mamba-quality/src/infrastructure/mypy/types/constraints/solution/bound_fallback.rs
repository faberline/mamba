//! A declared bound can replace a joined solution when it satisfies each input.
use super::*;

impl Context {
    pub(super) fn prevalidate_bound(
        &self,
        variable: &Variable,
        actual: Type,
        bounds: &Bounds,
    ) -> Type {
        let bound = self.call_variable_bound(variable);
        if self.accepts(bound, actual)
            || self.class_id(bound).is_some_and(|id| {
                self.class(id).protocol && self.protocol_members(id).contains("__call__")
            })
        {
            return actual;
        }
        if bounds.lower.iter().all(|&lower| self.accepts(bound, lower))
            && bounds
                .upper
                .iter()
                .chain(&bounds.exact)
                .all(|&upper| self.accepts(upper, bound))
            && bounds.exact.iter().all(|&exact| self.accepts(bound, exact))
        {
            bound
        } else {
            actual
        }
    }
}
