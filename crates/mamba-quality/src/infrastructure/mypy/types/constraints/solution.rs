//! Solve collected bounds without changing their declaration order.
use super::super::variables::Variable;
use super::*;
mod bound_fallback;

impl Context {
    pub(in super::super) fn solve_method_receiver(
        &self,
        variables: &[Type],
        template: Type,
        receiver: Type,
    ) -> Solution {
        let mut bounds = BTreeMap::new();
        self.constraints(template, receiver, Variance::Covariant, false, &mut bounds);
        self.solve_bounds_with_declaration(variables, bounds, false)
    }

    pub(super) fn solve_bounds(
        &self,
        variables: &[Type],
        bounds: BTreeMap<Type, Bounds>,
    ) -> Solution {
        self.solve_bounds_with_declaration(variables, bounds, true)
    }

    fn solve_bounds_with_declaration(
        &self,
        variables: &[Type],
        bounds: BTreeMap<Type, Bounds>,
        validate_declaration: bool,
    ) -> Solution {
        let mut solution = Solution {
            types: BTreeMap::new(),
            unresolved: Vec::new(),
            failed: Vec::new(),
            invalid: Vec::new(),
            factory_poly: None,
        };
        for &ty in variables {
            let node = self.node(ty).expect("declared type variable");
            let Node::Variable(variable) = node.as_ref() else {
                continue;
            };
            let default = validate_declaration
                .then(|| {
                    variable
                        .default
                        .or_else(|| self.classes.borrow().inference_defaults.get(&ty).copied())
                })
                .flatten();
            let Some(bounds) = bounds.get(&ty) else {
                if default.is_none() {
                    solution.unresolved.push(ty);
                }
                solution.types.insert(
                    ty,
                    self.substitute(default.unwrap_or(Type::Never), &solution.types),
                );
                continue;
            };
            // solve_one derives a fresh Any from a whole Any top/bottom.
            // Class argument substitution does not use this method-generic solve.
            let special_top = bounds
                .upper
                .contains(&Type::AnySpecial)
                .then(|| {
                    bounds
                        .upper
                        .iter()
                        .copied()
                        .reduce(|left, right| self.meet_parameter_types(left, right))
                })
                .flatten();
            if variable.parameter_spec {
                let actual = bounds
                    .lower
                    .iter()
                    .copied()
                    .reduce(|left, right| self.join_parameter_bounds(left, right))
                    .or_else(|| {
                        bounds
                            .upper
                            .iter()
                            .copied()
                            .reduce(|left, right| self.meet_parameter_bounds(left, right))
                    })
                    .unwrap_or(default.unwrap_or(Type::Never))
                    .derived_any();
                let actual = if special_top.is_some_and(Type::is_any) {
                    Type::Any
                } else {
                    actual
                };
                if bounds
                    .upper
                    .iter()
                    .chain(&bounds.exact)
                    .all(|&upper| self.accepts(upper, actual))
                {
                    solution.types.insert(ty, actual);
                } else {
                    solution.failed.push(ty);
                    solution.types.insert(ty, Type::Any);
                }
                continue;
            }
            let actual = bounds
                .lower
                .iter()
                .copied()
                .reduce(|left, right| {
                    if variable.type_var_tuple {
                        self.join_tuple_bounds(left, right)
                    } else {
                        self.join(left, right)
                    }
                })
                .or_else(|| {
                    bounds.upper.iter().copied().reduce(|left, right| {
                        if variable.type_var_tuple {
                            self.meet_tuple_bounds(left, right)
                        } else if self.accepts(left, right) {
                            right
                        } else if self.accepts(right, left) {
                            left
                        } else {
                            Type::Never
                        }
                    })
                })
                .unwrap_or_else(|| {
                    self.substitute(default.unwrap_or(Type::Never), &solution.types)
                });
            let actual = if actual == Type::AnySpecial || special_top.is_some_and(Type::is_any) {
                Type::Any
            } else {
                actual
            };
            let actual = if validate_declaration {
                self.prevalidate_bound(variable, actual, bounds)
            } else {
                actual
            };
            if variable.type_var_tuple && bounds.lower.is_empty() {
                solution.types.insert(ty, actual);
                continue;
            }
            if !bounds
                .exact
                .iter()
                .chain(&bounds.upper)
                .all(|&upper| self.accepts(upper, actual))
            {
                solution.failed.push(ty);
                solution.types.insert(ty, Type::Any);
                continue;
            }
            let mut replacement = actual;
            if validate_declaration && !actual.is_any() && !variable.values.is_empty() {
                let mut best = None;
                for &value in &variable.values {
                    if self.accepts(value, actual)
                        && best.is_none_or(|best| self.accepts(best, value))
                    {
                        best = Some(value);
                    }
                }
                if let Some(best) = best {
                    replacement = best;
                } else {
                    solution.invalid.push((ty, actual));
                }
            } else if validate_declaration
                && !self.accepts(self.call_variable_bound(variable), actual)
            {
                solution.invalid.push((ty, actual));
            }
            solution.types.insert(ty, replacement);
        }
        solution
    }

    fn call_variable_bound(&self, variable: &Variable) -> Type {
        if variable.name != "Self" {
            return variable.bound;
        }
        let mut variables = Vec::new();
        self.collect_variables(variable.bound, &mut variables);
        self.substitute(
            variable.bound,
            &variables.into_iter().map(|ty| (ty, Type::Any)).collect(),
        )
    }
}
