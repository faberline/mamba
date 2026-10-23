//! Awaitable constraints preserve yield and result covariance and send contravariance.
use super::*;

impl Context {
    pub(super) fn asynchronous_constraints(
        &self,
        kind: Container,
        expected: &[Type],
        actual: Type,
        variance: Variance,
        contextual: bool,
        bounds: &mut BTreeMap<Type, Bounds>,
    ) {
        if kind == Container::Awaitable {
            if let Some(result) = self.await_result(actual) {
                self.constraints(expected[0], result, variance, contextual, bounds);
            }
            return;
        }
        if kind == Container::Iterator && self.class_id(actual).is_some() {
            if let Some(item) = self.iterator_item(actual) {
                self.constraints(expected[0], item, variance, contextual, bounds);
            }
            return;
        }
        let actual_node = self.node(actual);
        let Some(Node::Container(other, actual)) = actual_node.as_deref() else {
            return;
        };
        if kind == Container::Coroutine && *other == Container::Awaitable && contextual {
            self.constraints(expected[2], actual[0], variance, contextual, bounds);
            return;
        }
        if kind == *other
            || (kind == Container::Iterator && *other == Container::Generator)
            || (matches!(kind, Container::AsyncIterator | Container::AsyncIterable)
                && matches!(other, Container::AsyncIterator | Container::AsyncGenerator))
        {
            for (index, (&expected, &actual)) in expected.iter().zip(actual).enumerate() {
                self.constraints(
                    expected,
                    actual,
                    if index == 1 {
                        variance.reverse()
                    } else {
                        variance
                    },
                    contextual,
                    bounds,
                );
            }
        }
    }
}
