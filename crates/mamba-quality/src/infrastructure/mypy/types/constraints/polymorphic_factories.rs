//! The single unconstrained factory leaf has a bounded polymorphic fallback.
use super::*;
use crate::infrastructure::mypy::signatures::Signature;

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct FactoryAmbiguity {
    pub factory: Type,
    pub overload: Option<usize>,
    pub foreign_binder: Type,
    pub value_binder: Type,
    pub raw_value: Type,
    pub ambiguous_value: Type,
    pub unconstrained_callee: Vec<Type>,
}

impl Context {
    pub(in crate::infrastructure::mypy) fn apply_linear_factory_poly(
        &self,
        signature: &Signature,
        arguments: &[(Type, Type)],
        solution: &mut Solution,
    ) {
        if let Some(receipt) = self.linear_factory_poly_receipt(signature, arguments, solution) {
            solution
                .types
                .insert(receipt.value_binder, receipt.ambiguous_value);
            solution.factory_poly = Some(receipt);
        }
    }

    fn linear_factory_poly_receipt(
        &self,
        signature: &Signature,
        arguments: &[(Type, Type)],
        regular: &Solution,
    ) -> Option<FactoryAmbiguity> {
        // Regular inference must first have a genuine unconstrained callee failure.
        if !signature.constructor
            || !regular.failed.is_empty()
            || !regular.invalid.is_empty()
            || regular.factory_poly.is_some()
            || regular.unresolved.is_empty()
        {
            return None;
        }
        let [(input_expected, factory)] = arguments else {
            return None;
        };
        if signature
            .generic
            .iter()
            .any(|&variable| !self.free_factory_variable(variable))
            || signature
                .generic
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != signature.generic.len()
            || regular
                .unresolved
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != regular.unresolved.len()
        {
            return None;
        }
        let returned = signature.returns?;
        let node = self.node(returned)?;
        let Node::AppliedClass {
            id,
            arguments: returned_arguments,
            value: false,
        } = node.as_ref()
        else {
            return None;
        };
        if returned_arguments.as_slice() != signature.generic.as_ref() {
            return None;
        }
        let class = self.class(*id);
        if class.plugin || !class.any_base_origins.is_empty() {
            return None;
        }
        let target = self
            .classes
            .borrow()
            .module_targets
            .get(&class.namespace)
            .copied()?;
        let node = self.node(*input_expected)?;
        let Node::Union(items) = node.as_ref() else {
            return None;
        };
        if items.len() != 2 || !items.contains(&Type::None) {
            return None;
        }
        let expected = *items.iter().find(|&&item| item != Type::None)?;
        let node = self.node(expected)?;
        let Node::Callable(callback) = node.as_ref() else {
            return None;
        };
        if !callback.parameters.is_empty()
            || callback.ellipsis
            || !callback.generic.is_empty()
            || !callback.overloads.is_empty()
            || callback.narrowing.is_some()
        {
            return None;
        }
        let value_binder = callback.returns?;
        if !signature.generic.contains(&value_binder)
            || regular.unresolved.contains(&value_binder)
            || signature.generic.len() != regular.unresolved.len() + 1
            || regular.unresolved.iter().any(|variable| {
                !signature.generic.contains(variable)
                    || regular.types.get(variable) != Some(&Type::Never)
            })
        {
            return None;
        }
        let node = self.node(*factory)?;
        let Node::Callable(actual) = node.as_ref() else {
            return None;
        };
        let selected = if actual.overloads.is_empty() {
            (None, actual.as_ref())
        } else {
            actual
                .overloads
                .iter()
                .enumerate()
                .find_map(|(index, variant)| {
                    crate::infrastructure::mypy::signatures::compatibility::parameters_accept(
                        self, callback, variant, true,
                    )
                    .then_some((Some(index), variant.as_ref()))
                })?
        };
        if !selected.1.constructor
            || !selected.1.parameters.is_empty()
            || selected.1.ellipsis
            || selected.1.narrowing.is_some()
            || selected.1.generic.len() != 1
            || !crate::infrastructure::mypy::signatures::compatibility::parameters_accept(
                self, callback, selected.1, true,
            )
        {
            return None;
        }
        let foreign_binder = selected.1.generic[0];
        if signature.generic.contains(&foreign_binder)
            || !self.free_factory_variable(foreign_binder)
        {
            return None;
        }
        let raw_value = selected.1.returns?;
        let node = self.node(raw_value)?;
        let Node::Container(kind @ (Container::List | Container::Set), items) = node.as_ref()
        else {
            return None;
        };
        if items.as_slice() != [foreign_binder]
            || regular.types.get(&value_binder) != Some(&raw_value)
        {
            return None;
        }
        let name = if *kind == Container::List {
            "list"
        } else {
            "set"
        };
        let key = format!("{target:?}:constructor:{name}");
        let canonical = self
            .classes
            .borrow()
            .collection_functions
            .get(&key)
            .cloned()?;
        if canonical.as_ref() != actual.as_ref() {
            return None;
        }

        // Poly retry: the entire dependency graph is V >= family[T], plus free leaves.
        // No unexamined constant, reverse, or additional-variable bound is admitted.
        let mut bounds = BTreeMap::new();
        self.constraints(
            *input_expected,
            *factory,
            Variance::Covariant,
            false,
            &mut bounds,
        );
        if bounds.len() != 1 {
            return None;
        }
        let value_bounds = bounds.get(&value_binder)?;
        if value_bounds.lower.is_empty()
            || value_bounds.lower.iter().any(|&ty| ty != raw_value)
            || !value_bounds.upper.is_empty()
            || !value_bounds.exact.is_empty()
        {
            return None;
        }

        // apply_poly cannot bind a free T in a zero-argument callback's return.
        // Its translator collects binders only from callable argument types.
        let ambiguous_value = self.intern(Node::Container(*kind, vec![Type::Never]));
        Some(FactoryAmbiguity {
            factory: *factory,
            overload: selected.0,
            foreign_binder,
            value_binder,
            raw_value,
            ambiguous_value,
            unconstrained_callee: regular.unresolved.clone(),
        })
    }

    fn free_factory_variable(&self, ty: Type) -> bool {
        matches!(self.node(ty).as_deref(), Some(Node::Variable(variable))
            if variable.bound == Type::Object
                && variable.default.is_none()
                && variable.values.is_empty()
                && variable.variance == ParameterVariance::Invariant
                && !variable.parameter_spec
                && !variable.type_var_tuple)
            && !self.classes.borrow().inference_defaults.contains_key(&ty)
    }

    pub(in crate::infrastructure::mypy) fn factory_ambiguity_matches(
        &self,
        signature: &Signature,
        solution: &Solution,
        receipt: &FactoryAmbiguity,
    ) -> bool {
        let Some(node) = self.node(receipt.factory) else {
            return false;
        };
        let Node::Callable(factory) = node.as_ref() else {
            return false;
        };
        let selected = match receipt.overload {
            Some(index) => factory.overloads.get(index).map(|variant| variant.as_ref()),
            None if factory.overloads.is_empty() => Some(factory.as_ref()),
            _ => None,
        };
        let Some(selected) = selected else {
            return false;
        };
        selected.generic.as_ref() == [receipt.foreign_binder]
            && selected.returns == Some(receipt.raw_value)
            && signature.generic.contains(&receipt.value_binder)
            && self.free_factory_variable(receipt.foreign_binder)
            && solution.types.get(&receipt.value_binder) == Some(&receipt.ambiguous_value)
            && solution.unresolved == receipt.unconstrained_callee
            && receipt.unconstrained_callee.iter().all(|variable| {
                signature.generic.contains(variable)
                    && solution.types.get(variable) == Some(&Type::Never)
            })
            && matches!((self.node(receipt.raw_value).as_deref(), self.node(receipt.ambiguous_value).as_deref()),
                (Some(Node::Container(left @ (Container::List | Container::Set), original)),
                 Some(Node::Container(right, erased)))
                    if left == right && original.as_slice() == [receipt.foreign_binder]
                        && erased.as_slice() == [Type::Never])
    }
}
