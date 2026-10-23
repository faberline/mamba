//! Infer ordinary inputs before instantiating polymorphic callback values.
use super::*;
mod parameter_specs;

impl Context {
    pub(super) fn solve_higher_order(
        &self,
        variables: &[Type],
        arguments: &[(Type, Type)],
    ) -> constraints::Solution {
        let mut prepared: Option<Vec<(Type, Type)>> = None;
        for (index, &(expected, actual)) in arguments.iter().enumerate() {
            let instantiated = self
                .union_callback_target(expected, actual)
                .and_then(|target| {
                    let signature = self.callable_signature(actual)?;
                    let target = self.callable_signature(target)?;
                    self.instantiate_union_callback(signature.as_ref(), target.as_ref())
                });
            if let Some(signature) = instantiated {
                prepared.get_or_insert_with(|| arguments.to_vec())[index].1 =
                    self.callable_type(std::sync::Arc::new(signature));
            }
        }
        let arguments = prepared.as_deref().unwrap_or(arguments);
        let callback = |expected, actual| {
            self.node(expected)
                .is_some_and(|node| matches!(node.as_ref(), Node::Callable(_)))
                && self.callable_signature(actual).is_some()
        };
        if !arguments
            .iter()
            .any(|&(expected, actual)| callback(expected, actual))
        {
            return self.solve(variables, arguments);
        }
        let ordinary: Vec<_> = arguments
            .iter()
            .copied()
            .filter(|&(expected, actual)| !callback(expected, actual))
            .collect();
        let partial = self.solve(variables, &ordinary);
        let known: BTreeMap<_, _> = partial
            .types
            .into_iter()
            .filter(|&(variable, ty)| ty != Type::Never && !self.is_parameter_spec(variable))
            .collect();
        let instantiated: Vec<_> = arguments
            .iter()
            .map(|&(expected, actual)| {
                if callback(expected, actual) {
                    let expected = self.substitute(expected, &known);
                    let target = self.callable_signature(expected).unwrap();
                    let signature = self.callable_signature(actual).unwrap();
                    if !signature.generic.is_empty() {
                        if let Some(signature) =
                            self.rebind_parameter_spec_callback(&signature, &target)
                        {
                            return (expected, self.callable_type(std::sync::Arc::new(signature)));
                        }
                        if let Some(signature) = self.instantiate_callable(&signature, &target) {
                            return (expected, self.callable_type(std::sync::Arc::new(signature)));
                        }
                    }
                    return (expected, actual);
                }
                (expected, actual)
            })
            .collect();
        let mut solution = self.solve(variables, &instantiated);
        solution.types.extend(known);
        solution
    }

    fn union_callback_target(&self, expected: Type, actual: Type) -> Option<Type> {
        let node = self.node(expected)?;
        let Node::Union(items) = node.as_ref() else {
            return None;
        };
        if matches!(
            actual,
            Type::Any | Type::AnySpecial | Type::None | Type::Never
        ) {
            return None;
        }
        let signature = self.callable_signature(actual)?;
        if signature.generic.is_empty()
            && signature
                .overloads
                .iter()
                .all(|variant| variant.generic.is_empty())
        {
            return None;
        }
        // Use the same admission order as Union constraint collection.
        if items.iter().any(|&item| {
            let mut variables = Vec::new();
            self.collect_variables(item, &mut variables);
            variables.is_empty() && self.accepts(item, actual)
        }) {
            return None;
        }
        let mut candidates = items
            .iter()
            .copied()
            .filter(|&item| self.constraint_shape_matches(item, actual));
        let target = candidates.next()?;
        if candidates.next().is_some()
            || !self
                .node(target)
                .is_some_and(|node| matches!(node.as_ref(), Node::Callable(_)))
        {
            return None;
        }
        Some(target)
    }

    pub(crate) fn accepts_callable(
        &self,
        parent: &super::super::signatures::Signature,
        child: &super::super::signatures::Signature,
        ignore_names: bool,
    ) -> bool {
        if !parent.overloads.is_empty() && !child.overloads.is_empty() {
            return self.accepts_overloads(parent, child, ignore_names);
        }
        if !parent.overloads.is_empty() {
            return parent
                .overloads
                .iter()
                .all(|parent| self.accepts_callable(parent, child, ignore_names));
        }
        if !child.overloads.is_empty() {
            return child
                .overloads
                .iter()
                .any(|child| self.accepts_callable(parent, child, ignore_names))
                || self
                    .combine_overloads(child)
                    .is_some_and(|child| self.accepts_callable(parent, &child, ignore_names));
        }
        let instantiated = if !parent.generic.is_empty() && !child.generic.is_empty() {
            let Some(signature) = self.align_callable_variables(parent, child) else {
                return false;
            };
            Some(signature)
        } else if child.generic.is_empty() {
            None
        } else {
            let Some(signature) = self.instantiate_callable(child, parent) else {
                return false;
            };
            Some(signature)
        };
        super::super::signatures::compatibility::accepts(
            self,
            parent,
            instantiated.as_ref().unwrap_or(child),
            ignore_names,
        )
    }
}
