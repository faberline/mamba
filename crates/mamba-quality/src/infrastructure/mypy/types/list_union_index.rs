//! Read indexing retains bounded list and dynamic-key dictionary union paths.
use super::*;

impl Context {
    pub(super) fn list_union_index_result(&self, items: &[Type], slice: bool) -> Option<Type> {
        if items.is_empty() {
            return None;
        }
        if !slice && items.iter().any(|ty| ty.is_any()) {
            let mut results = Vec::with_capacity(items.len());
            for &receiver in items {
                if receiver.is_any() {
                    results.push(Type::Any);
                    continue;
                }
                match self.node(receiver).as_deref() {
                    Some(Node::Container(Container::Dict, arguments))
                        if arguments.len() == 2 && arguments[0].is_any() =>
                    {
                        results.push(arguments[1]);
                    }
                    _ => return None,
                }
            }
            return Some(self.union(results));
        }
        let mut results = Vec::with_capacity(items.len());
        for &receiver in items {
            let node = self.node(receiver)?;
            let Node::Container(Container::List, arguments) = node.as_ref() else {
                return None;
            };
            if arguments.len() != 1 {
                return None;
            }
            results.push(if slice {
                receiver
            } else {
                self.sequence_item(receiver)?
            });
        }
        Some(self.union(results))
    }

    // None is an invalid receiver, not a missing inference result. Its checker
    // diagnostic remains mandatory; only proven dictionary arms supply values.
    pub(super) fn optional_dict_index_result(
        &self,
        items: &[Type],
        slice: bool,
        index_type: impl FnOnce() -> Option<Type>,
    ) -> Option<Type> {
        if slice || !items.contains(&Type::None) {
            return None;
        }
        let mut dictionaries = Vec::new();
        for &receiver in items {
            if receiver == Type::None {
                continue;
            }
            let node = self.node(receiver)?;
            let Node::Container(Container::Dict, arguments) = node.as_ref() else {
                return None;
            };
            if arguments.len() != 2 || arguments[0] == Type::Never {
                return None;
            }
            dictionaries.push((arguments[0], arguments[1]));
        }
        if dictionaries.is_empty() {
            return None;
        }
        let index = index_type()?;
        if index == Type::Never
            || dictionaries
                .iter()
                .any(|&(key, _)| !self.accepts(key, index))
        {
            return None;
        }
        let mut values = dictionaries.into_iter().map(|(_, value)| value);
        Some(
            self.union(
                items
                    .iter()
                    .map(|&receiver| {
                        if receiver == Type::None {
                            Type::Any
                        } else {
                            values.next().expect("proven dictionary arm")
                        }
                    })
                    .collect(),
            ),
        )
    }
}
