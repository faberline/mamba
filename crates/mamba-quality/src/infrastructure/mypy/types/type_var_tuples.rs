//! Tuple packs retain each element and one symbolic or homogeneous expansion.
use super::*;
mod bounds;
mod callables;
mod declarations;
mod narrowing;

impl Context {
    pub(super) fn default_generic_argument(&self, parameter: Type) -> Type {
        if self.is_tuple_variable(parameter) {
            self.intern(Node::VariadicTuple(Type::Any))
        } else {
            Type::Any
        }
    }
    pub(in crate::infrastructure::mypy) fn is_tuple_variable(&self, ty: Type) -> bool {
        self.node(ty).is_some_and(
            |node| matches!(node.as_ref(), Node::Variable(variable) if variable.type_var_tuple),
        )
    }

    pub(in crate::infrastructure::mypy) fn unpack_target(&self, ty: Type) -> Option<Type> {
        match self.node(ty)?.as_ref() {
            Node::Unpack(target) => Some(*target),
            _ => None,
        }
    }

    pub(in crate::infrastructure::mypy) fn normalize_tuple(&self, items: Vec<Type>) -> Type {
        let mut flattened = Vec::new();
        for item in items {
            if let Some(target) = self.unpack_target(item) {
                match self.node(target).as_deref() {
                    Some(Node::Tuple(items)) => {
                        flattened.extend(items.iter().copied());
                        continue;
                    }
                    _ if target.is_any() => {
                        flattened.push(
                            self.intern(Node::Unpack(self.intern(Node::VariadicTuple(target)))),
                        );
                        continue;
                    }
                    _ => {}
                }
            }
            flattened.push(item);
        }
        if let [item] = flattened.as_slice() {
            if let Some(target) = self.unpack_target(*item) {
                if self
                    .node(target)
                    .is_some_and(|node| matches!(node.as_ref(), Node::VariadicTuple(_)))
                {
                    return target;
                }
            }
        }
        self.intern(Node::Tuple(flattened))
    }

    pub(in crate::infrastructure::mypy) fn tuple_pack_position(
        &self,
        items: &[Type],
    ) -> Option<(usize, Type)> {
        items
            .iter()
            .enumerate()
            .find_map(|(index, &item)| self.unpack_target(item).map(|target| (index, target)))
    }

    pub(super) fn tuple_pattern_accepts(&self, expected: &[Type], actual: &[Type]) -> bool {
        let Some((prefix, target)) = self.tuple_pack_position(expected) else {
            return expected.len() == actual.len()
                && expected
                    .iter()
                    .zip(actual)
                    .all(|(&expected, &actual)| self.accepts(expected, actual));
        };
        let suffix = expected.len() - prefix - 1;
        if actual.len() < prefix + suffix {
            return false;
        }
        if !expected[..prefix]
            .iter()
            .zip(actual)
            .all(|(&expected, &actual)| self.accepts(expected, actual))
        {
            return false;
        }
        if !expected[prefix + 1..]
            .iter()
            .zip(&actual[actual.len() - suffix..])
            .all(|(&expected, &actual)| self.accepts(expected, actual))
        {
            return false;
        }
        self.accepts(
            target,
            self.intern(Node::Tuple(actual[prefix..actual.len() - suffix].to_vec())),
        )
    }

    pub(in crate::infrastructure::mypy) fn pack_generic_arguments(
        &self,
        parameters: &[Type],
        arguments: Vec<Type>,
    ) -> Vec<Type> {
        let Some(index) = parameters
            .iter()
            .position(|&parameter| self.is_tuple_variable(parameter))
        else {
            return arguments;
        };
        let suffix = parameters.len() - index - 1;
        let start = index.min(arguments.len());
        let end = arguments.len().saturating_sub(suffix).max(start);
        let packed = self.normalize_tuple(arguments[start..end].to_vec());
        arguments[..start]
            .iter()
            .copied()
            .chain(std::iter::once(packed))
            .chain(arguments[end..].iter().copied())
            .collect()
    }

    pub(in crate::infrastructure::mypy) fn generic_argument_count_valid(
        &self,
        parameters: &[Type],
        count: usize,
        minimum: usize,
    ) -> bool {
        let tuple_count = parameters
            .iter()
            .filter(|&&ty| self.is_tuple_variable(ty))
            .count();
        if tuple_count != 0 {
            count >= minimum.saturating_sub(tuple_count)
        } else {
            count >= minimum && count <= parameters.len()
        }
    }

    pub(in crate::infrastructure::mypy) fn display_generic_arguments(
        &self,
        parameters: &[Type],
        arguments: &[Type],
        reveal: bool,
    ) -> String {
        let name = |ty| {
            if reveal {
                self.revealed_name(ty)
            } else {
                self.name(ty)
            }
        };
        let mut names = Vec::new();
        for (index, &argument) in arguments.iter().enumerate() {
            if parameters
                .get(index)
                .is_some_and(|&parameter| self.is_tuple_variable(parameter))
            {
                match self.node(argument).as_deref() {
                    Some(Node::Tuple(items)) if items.is_empty() => {
                        if parameters.len() == 1 {
                            names.push("()".into());
                        }
                    }
                    Some(Node::Tuple(items)) => names.extend(items.iter().map(|&item| name(item))),
                    Some(Node::VariadicTuple(_)) => names.push(format!("*{}", name(argument))),
                    _ => names.push(format!("*{}", name(argument))),
                }
            } else {
                names.push(name(argument));
            }
        }
        names.join(", ")
    }
}
