//! Tuple-like inputs contribute one constraint for each declared element.
use super::*;

impl Context {
    pub(super) fn tuple_iterable_constraints(
        &self,
        expected: Type,
        actual: Type,
        variance: Variance,
        contextual: bool,
        bounds: &mut BTreeMap<Type, Bounds>,
    ) -> bool {
        if contextual {
            return false;
        }
        let Some(Node::Tuple(items)) = self.node(actual).as_deref().cloned() else {
            return false;
        };
        let item = match self.node(expected).as_deref() {
            Some(Node::Container(Container::Iterable | Container::Sequence, arguments)) => {
                arguments.first().copied()
            }
            Some(Node::VariadicTuple(item)) => Some(*item),
            _ => self.class_id(expected).and_then(|id| {
                matches!(
                    self.class(id).namespace.as_str(),
                    "builtins.tuple"
                        | "typing.Iterable"
                        | "typing.Container"
                        | "typing.Sequence"
                        | "typing.Reversible"
                )
                .then(|| self.class_arguments(expected).first().copied())
                .flatten()
            }),
        };
        let Some(item) = item else { return false };
        if items.is_empty() {
            self.constraints(item, Type::Never, variance, contextual, bounds);
        }
        for actual in items {
            self.constraints(item, actual, variance, contextual, bounds);
        }
        true
    }
}
