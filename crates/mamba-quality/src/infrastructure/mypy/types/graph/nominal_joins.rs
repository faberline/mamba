//! Nominal joins with complete, non-generic single-inheritance evidence.
use super::*;

impl Context {
    pub(super) fn single_inheritance_join(&self, left: Type, right: Type) -> Option<Type> {
        let left = self.single_inheritance_lineage(left)?;
        let right = self.single_inheritance_lineage(right)?;
        let common = left.into_iter().find(|id| right.contains(id))?;
        let class = self.class(common);
        // Keep the existing scalar object result when no narrower base exists.
        class.scalar_bases.is_empty().then_some(class.instance)
    }

    fn single_inheritance_lineage(&self, ty: Type) -> Option<Vec<u32>> {
        let node = self.node(ty)?;
        let Node::Instance(id) = node.as_ref() else {
            return None;
        };
        let mut id = *id;
        let mut lineage = Vec::new();
        loop {
            if lineage.contains(&id)
                || self.classes.borrow().populated_class_bases.get(&id) != Some(&true)
            {
                return None;
            }
            let class = self.class(id);
            if !class.generic.is_empty()
                || !class.any_base_origins.is_empty()
                || class.plugin
                || class.protocol
                || class.typed_dict.is_some()
                || !matches!(self.node(class.instance).as_deref(), Some(Node::Instance(owner)) if *owner == id)
            {
                return None;
            }
            lineage.push(id);
            if class.bases.is_empty()
                && class.base_types.is_empty()
                && matches!(class.scalar_bases.as_slice(), [Type::Object])
            {
                return Some(lineage);
            }
            let ([base], [base_type], []) = (
                class.bases.as_slice(),
                class.base_types.as_slice(),
                class.scalar_bases.as_slice(),
            ) else {
                return None;
            };
            if !matches!(self.node(*base_type).as_deref(), Some(Node::Instance(owner)) if owner == base)
            {
                return None;
            }
            id = *base;
        }
    }
}
