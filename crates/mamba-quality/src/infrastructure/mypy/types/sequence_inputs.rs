//! Nominal sequence bases retain their declared item type after specialization.
use super::*;

impl Context {
    pub(super) fn nominal_sequence_item(&self, ty: Type) -> Option<Type> {
        if self.is_class_value(ty) {
            return None;
        }
        for id in self.class_lineage(self.class_id(ty)?) {
            let class = self.class(id);
            let solutions = self.class_solutions(ty, id);
            for &base in &class.base_types {
                let base = self.substitute(base, &solutions);
                match self.node(base).as_deref() {
                    Some(Node::Container(Container::Sequence | Container::List, arguments)) => {
                        return Some(arguments[0])
                    }
                    Some(Node::VariadicTuple(item)) => return Some(*item),
                    _ => {}
                }
            }
        }
        None
    }
}
