//! Built-in collection inputs share nominal parameter variance.
use super::*;
mod tuple_inputs;

impl Context {
    pub(super) fn collection_constraints(
        &self,
        expected: Type,
        actual: Type,
        variance: Variance,
        contextual: bool,
        bounds: &mut BTreeMap<Type, Bounds>,
    ) -> bool {
        if self.tuple_iterable_constraints(expected, actual, variance, contextual, bounds) {
            return true;
        }
        if contextual {
            if let (
                Some(Node::Container(Container::List, items)),
                Some(Node::Container(Container::Sequence | Container::Iterable, arguments)),
            ) = (self.node(expected).as_deref(), self.node(actual).as_deref())
            {
                self.constraints(items[0], arguments[0], variance, contextual, bounds);
                return true;
            }
            if let (
                Some(Node::Container(Container::Dict, items)),
                Some(Node::Container(Container::Mapping, arguments)),
            ) = (self.node(expected).as_deref(), self.node(actual).as_deref())
            {
                for (&expected, &actual) in items.iter().zip(arguments) {
                    self.constraints(expected, actual, variance, contextual, bounds);
                }
                return true;
            }
            if let Some(expected_id) = self.class_id(expected) {
                if let Some(actual_id) = self.class_id(actual) {
                    if self.nominal_accepts(expected_id, actual_id) {
                        if let Some(arguments) = self.class_arguments_for(actual, expected_id) {
                            for (expected, actual) in
                                self.class_arguments(expected).into_iter().zip(arguments)
                            {
                                self.constraints(expected, actual, variance, contextual, bounds);
                            }
                            return true;
                        }
                    }
                }
            }
            if let Some(Node::Container(kind @ (Container::Set | Container::FrozenSet), items)) =
                self.node(expected).as_deref()
            {
                if let Some(id) = self.class_id(actual) {
                    let class = self.class(id);
                    let accepted = match class.namespace.as_str() {
                        "typing.AbstractSet" => true,
                        "typing.MutableSet" => *kind == Container::Set,
                        "typing.Container" | "typing.Collection" | "typing.Iterable" => true,
                        _ => false,
                    };
                    if accepted {
                        if let Some(item) = self.class_arguments(actual).first() {
                            self.constraints(items[0], *item, variance, contextual, bounds);
                            return true;
                        }
                    }
                }
            }
        }
        if let Some(id) = self
            .class_id(expected)
            .filter(|_| !self.is_class_value(expected))
        {
            if let Some(actual) = self.builtin_collection_arguments(actual, id) {
                for (index, (expected, actual)) in self
                    .class_arguments(expected)
                    .into_iter()
                    .zip(actual)
                    .enumerate()
                {
                    let direction = match self.class_parameter_variance(id, index) {
                        ParameterVariance::Covariant => variance,
                        ParameterVariance::Contravariant => variance.reverse(),
                        ParameterVariance::Invariant | ParameterVariance::Inferred => {
                            Variance::Invariant
                        }
                    };
                    self.constraints(expected, actual, direction, contextual, bounds);
                }
                return true;
            }
        }
        false
    }
}
