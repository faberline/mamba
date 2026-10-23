//! Iterable input conflicts retain the expected and actual iterator element types.
use super::*;

impl Context {
    pub(super) fn iterable_hint(&self, expected: Type, actual: Type) -> Option<String> {
        let node = self.node(expected)?;
        if let Node::Union(items) = node.as_ref() {
            return items
                .iter()
                .find_map(|&item| self.iterable_hint(item, actual));
        }
        let Node::Container(Container::Iterable, items) = node.as_ref() else {
            return None;
        };
        if self.typed_dict(actual).is_some() || items[0] == Type::Never {
            return None;
        }
        if self.node(actual).is_some_and(|node| {
            matches!(
                node.as_ref(),
                Node::Tuple(_) | Node::VariadicTuple(_) | Node::Container(_, _)
            )
        }) {
            return None;
        }
        let actual_item = self.iterable_item(actual)?;
        if self.class_id(actual).is_some_and(|id| {
            matches!(
                self.class(id).namespace.as_str(),
                "typing.Iterator" | "typing.Iterable"
            )
        }) {
            return None;
        }
        if self.accepts(items[0], actual_item) {
            return None;
        }
        Some(format!("Following member(s) of \"{}\" have conflicts:\n    Expected:\n        def __iter__(self) -> Iterator[{}]\n    Got:\n        def __iter__(self) -> Iterator[{}]", self.name(actual), self.name(items[0]), self.name(actual_item)))
    }
}
