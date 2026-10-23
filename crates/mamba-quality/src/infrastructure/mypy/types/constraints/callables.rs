//! Callable constraints retain parameter names, kinds, and default acceptance.
use super::super::super::signatures::Signature;
use super::*;

impl Context {
    pub(in super::super) fn instantiate_callable(
        &self,
        signature: &Signature,
        target: &Signature,
    ) -> Option<Signature> {
        let bounds = self.callable_instantiation_bounds(signature, target);
        let solution = self.solve_bounds(&signature.generic, bounds);
        (solution.failed.is_empty() && solution.invalid.is_empty())
            .then(|| self.substitute_callable(signature, &solution.types))
    }

    pub(in super::super) fn instantiate_union_callback(
        &self,
        signature: &Signature,
        target: &Signature,
    ) -> Option<Signature> {
        if !target.overloads.is_empty() {
            return None;
        }
        let selected = if signature.overloads.is_empty() {
            signature
        } else {
            signature
                .overloads
                .iter()
                .find(|variant| {
                    crate::infrastructure::mypy::signatures::compatibility::parameters_accept(
                        self, target, variant, true,
                    )
                })?
                .as_ref()
        };
        if selected.generic.is_empty() {
            return None;
        }
        let mut bounds = self.callable_instantiation_bounds(selected, target);
        // Rejected enclosing variables are not evidence for a bottom solution.
        bounds.retain(|_, bounds| {
            !bounds.lower.is_empty() || !bounds.upper.is_empty() || !bounds.exact.is_empty()
        });
        let solution = self.solve_bounds(&selected.generic, bounds);
        if !solution.unresolved.is_empty()
            || !solution.failed.is_empty()
            || !solution.invalid.is_empty()
        {
            return None;
        }
        let selected = self.substitute_callable(selected, &solution.types);
        if !crate::infrastructure::mypy::signatures::compatibility::accepts(
            self, target, &selected, true,
        ) {
            return None;
        }
        // Preserve every overload and substitute only proved binder solutions.
        Some(self.substitute_callable(signature, &solution.types))
    }

    fn callable_instantiation_bounds(
        &self,
        signature: &Signature,
        target: &Signature,
    ) -> BTreeMap<Type, Bounds> {
        let template = self.callable_type(std::sync::Arc::new(signature.clone()));
        let actual = self.callable_type(std::sync::Arc::new(target.clone()));
        let mut bounds = BTreeMap::new();
        self.constraints(template, actual, Variance::Covariant, false, &mut bounds);
        // An enclosing generic variable is not a solution for this binder.
        for constraints in bounds.values_mut() {
            for candidates in [
                &mut constraints.lower,
                &mut constraints.upper,
                &mut constraints.exact,
            ] {
                candidates.retain(|&ty| {
                    let mut variables = Vec::new();
                    self.collect_variables(ty, &mut variables);
                    variables.is_empty()
                });
            }
        }
        bounds
    }

    pub(super) fn callable_constraints(
        &self,
        expected: &Signature,
        actual: Type,
        variance: Variance,
        contextual: bool,
        bounds: &mut BTreeMap<Type, Bounds>,
    ) {
        let Some(actual) = self.callable_signature(actual) else {
            return;
        };
        let actual = if actual.overloads.is_empty() {
            actual
        } else {
            actual
                .overloads
                .iter()
                .find(|variant| {
                    crate::infrastructure::mypy::signatures::compatibility::parameters_accept(
                        self, expected, variant, true,
                    )
                })
                .unwrap_or(&actual.overloads[0])
                .clone()
        };
        let parameter_spec = self.signature_parameter_spec(expected);
        let tuple_pack =
            self.callable_tuple_constraints(expected, &actual, variance, contextual, bounds);
        if let Some((prefix, variable)) = parameter_spec {
            let prefix = prefix.min(
                actual
                    .parameters
                    .iter()
                    .filter(|parameter| {
                        matches!(
                            parameter.kind,
                            super::super::super::signatures::Kind::PositionalOnly
                                | super::super::super::signatures::Kind::PositionalOrKeyword
                        )
                    })
                    .count(),
            );
            let captured = if prefix == 0 {
                self.signature_parameter_spec(&actual)
                    .filter(|&(index, _)| index == 0)
                    .map(|(_, variable)| variable)
            } else {
                None
            }
            .unwrap_or_else(|| {
                self.intern(Node::Parameters(
                    actual.parameters.iter().skip(prefix).cloned().collect(),
                    actual.ellipsis,
                ))
            });
            let constraints = bounds.entry(variable).or_default();
            match variance {
                Variance::Covariant => constraints.lower.push(captured),
                Variance::Contravariant => constraints.upper.push(captured),
                Variance::Invariant => {
                    constraints.lower.push(captured);
                    constraints.exact.push(captured);
                }
            }
        }
        if !tuple_pack && !expected.ellipsis && !actual.ellipsis {
            for (expected, actual) in expected
                .parameters
                .iter()
                .take(parameter_spec.map_or(expected.parameters.len(), |(prefix, _)| prefix))
                .zip(actual.parameters.iter())
            {
                if let (Some(expected), Some(actual)) = (expected.ty, actual.ty) {
                    self.constraints(expected, actual, variance.reverse(), contextual, bounds);
                }
            }
        }
        if let (Some(expected), Some(actual)) = (expected.returns, actual.returns) {
            self.constraints(expected, actual, variance, contextual, bounds);
        }
    }
}
