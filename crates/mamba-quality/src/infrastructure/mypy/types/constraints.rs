//! Structural constraints infer substitutions independently from diagnostics.
use super::*;
mod aliases;
mod async_types;
mod callables;
mod collections;
mod equivalent_unions;
mod polymorphic_factories;
mod return_context;
mod shapes;
mod solution;
mod tuple_packs;
pub(crate) use polymorphic_factories::FactoryAmbiguity;

#[derive(Clone, Default, PartialEq, Eq)]
struct Bounds {
    lower: Vec<Type>,
    upper: Vec<Type>,
    exact: Vec<Type>,
}

#[derive(Clone, Copy)]
enum Variance {
    Covariant,
    Contravariant,
    Invariant,
}

impl Variance {
    fn reverse(self) -> Self {
        match self {
            Self::Covariant => Self::Contravariant,
            Self::Contravariant => Self::Covariant,
            Self::Invariant => Self::Invariant,
        }
    }
}

pub(crate) struct Solution {
    pub types: BTreeMap<Type, Type>,
    pub unresolved: Vec<Type>,
    pub failed: Vec<Type>,
    pub invalid: Vec<(Type, Type)>,
    pub factory_poly: Option<FactoryAmbiguity>,
}

impl Context {
    pub(crate) fn solve_call(
        &self,
        variables: &[Type],
        arguments: &[(Type, Type)],
        return_context: Option<(Type, Type)>,
    ) -> Solution {
        let mut fixed = BTreeMap::new();
        if let Some((template, target)) = return_context {
            let target = if self.alias_application(template).is_some() {
                self.simplify_inference_union(target)
            } else {
                target
            };
            if self
                .node(template)
                .is_some_and(|node| !matches!(node.as_ref(), Node::Variable(_)))
                || self.node(target).is_some_and(|node| {
                    matches!(
                        node.as_ref(),
                        Node::Container(Container::List | Container::Set | Container::Dict, _)
                    )
                })
                || self
                    .class_id(target)
                    .is_some_and(|id| self.class(id).namespace == "typing.Iterator")
            {
                let mut bounds = BTreeMap::new();
                if !self.superclass_return_context_constraints(template, target, &mut bounds) {
                    self.constraints(template, target, Variance::Covariant, true, &mut bounds);
                }
                for (&variable, bounds) in &bounds {
                    if let Some(ty) = bounds
                        .lower
                        .iter()
                        .copied()
                        .reduce(|left, right| self.join(left, right))
                        .or_else(|| {
                            self.is_parameter_spec(variable)
                                .then(|| bounds.upper.first().copied())
                                .flatten()
                        })
                        .map(Type::derived_any)
                    {
                        if let Some(node) = self.node(variable) {
                            if let Node::Variable(declaration) = node.as_ref() {
                                if self.accepts(declaration.bound, ty)
                                    && (declaration.values.is_empty()
                                        || declaration.values.contains(&ty))
                                {
                                    fixed.insert(variable, ty);
                                }
                            }
                        }
                    }
                }
            }
        }
        let variables: Vec<_> = variables
            .iter()
            .copied()
            .filter(|ty| !fixed.contains_key(ty))
            .collect();
        let arguments: Vec<_> = arguments
            .iter()
            .map(|&(expected, actual)| (self.substitute(expected, &fixed), actual))
            .collect();
        let mut solution = self.solve_higher_order(&variables, &arguments);
        solution.types.extend(fixed);
        solution
    }

    pub(crate) fn solve(&self, variables: &[Type], arguments: &[(Type, Type)]) -> Solution {
        if variables.is_empty() {
            return Solution {
                types: BTreeMap::new(),
                unresolved: Vec::new(),
                failed: Vec::new(),
                invalid: Vec::new(),
                factory_poly: None,
            };
        }
        let mut bounds = BTreeMap::new();
        for &(expected, actual) in arguments {
            self.constraints(expected, actual, Variance::Covariant, false, &mut bounds);
        }
        self.solve_bounds(variables, bounds)
    }

    fn constraints(
        &self,
        expected: Type,
        actual: Type,
        variance: Variance,
        contextual: bool,
        bounds: &mut BTreeMap<Type, Bounds>,
    ) {
        if self.alias_constraints(expected, actual, variance, contextual, bounds) {
            return;
        }
        let Some(expected_node) = self.node(expected) else {
            return;
        };
        if matches!(expected_node.as_ref(), Node::Variable(_)) {
            let actual = if let Some(node) = self.node(actual) {
                if let Node::LiteralValue(value) = node.as_ref() {
                    value.base()
                } else {
                    actual
                }
            } else {
                actual
            };
            let constraints = bounds.entry(expected).or_default();
            match variance {
                Variance::Covariant => constraints.lower.push(actual),
                Variance::Contravariant => constraints.upper.push(actual),
                Variance::Invariant => {
                    constraints.lower.push(actual);
                    constraints.exact.push(actual);
                }
            }
            return;
        }
        let actual_node = self.node(actual);
        if actual.is_any() {
            let mut variables = Vec::new();
            self.collect_variables(expected, &mut variables);
            for variable in variables {
                bounds.entry(variable).or_default().lower.push(actual);
            }
            return;
        }
        // Only lower-bound unions require constraints from every actual arm.
        if matches!(variance, Variance::Covariant)
            && !matches!(expected_node.as_ref(), Node::Union(_))
        {
            if let Some(Node::Union(actuals)) = actual_node.as_deref() {
                for &actual in actuals {
                    self.constraints(expected, actual, variance, contextual, bounds);
                }
                return;
            }
        }
        if self.collection_constraints(expected, actual, variance, contextual, bounds) {
            return;
        }
        if let Some(id) = self
            .class_id(expected)
            .filter(|&id| self.class(id).protocol)
        {
            if !self.protocol_accepts(
                self.applied_class(id, vec![Type::Any; self.class(id).generic.len()], false),
                actual,
            ) {
                return;
            }
            let pair = (expected, actual);
            if !self
                .classes
                .borrow_mut()
                .protocol_constraint_checks
                .insert(pair)
            {
                return;
            }
            for (expected, actual, mutable) in self.protocol_constraint_members(expected, actual) {
                self.constraints(
                    expected,
                    actual,
                    if mutable {
                        Variance::Invariant
                    } else {
                        variance
                    },
                    contextual,
                    bounds,
                );
            }
            self.classes
                .borrow_mut()
                .protocol_constraint_checks
                .remove(&pair);
            return;
        }
        match (expected_node.as_ref(), actual_node.as_deref()) {
            (Node::Callable(expected), _) => {
                self.callable_constraints(expected, actual, variance, contextual, bounds)
            }
            (Node::Union(expected), actual_node) => {
                if self.recursive_union_constraints(expected, actual, variance, contextual, bounds)
                {
                    return;
                }
                let actuals = match actual_node {
                    Some(Node::Union(actuals)) => actuals.clone(),
                    _ => vec![actual],
                };
                for actual in actuals {
                    if expected.iter().any(|&expected| {
                        let mut variables = Vec::new();
                        self.collect_variables(expected, &mut variables);
                        variables.is_empty() && self.accepts(expected, actual)
                    }) {
                        continue;
                    }
                    let candidates: Vec<_> = expected
                        .iter()
                        .copied()
                        .filter(|&expected| self.constraint_shape_matches(expected, actual))
                        .collect();
                    if let [expected] = candidates.as_slice() {
                        // Union-to-union return context does not fix a bare type variable.
                        if actual_node.is_some_and(|node| matches!(node, Node::Union(_)))
                            && self
                                .node(*expected)
                                .is_some_and(|node| matches!(node.as_ref(), Node::Variable(_)))
                            && contextual
                        {
                            continue;
                        }
                        self.constraints(*expected, actual, variance, contextual, bounds);
                    } else if candidates.len() > 1 {
                        self.equivalent_callable_constraints(
                            &candidates,
                            actual,
                            variance,
                            contextual,
                            bounds,
                        );
                    }
                }
            }
            (Node::Container(Container::TypeObject, expected), _) => {
                if self.is_class_value(actual) {
                    if let Some(actual) = self.instance_type(actual) {
                        self.constraints(expected[0], actual, variance, contextual, bounds);
                    }
                }
            }
            (Node::Container(Container::Sequence, expected), _) => {
                if let Some(actual) = self.sequence_item(actual) {
                    self.constraints(expected[0], actual, variance, contextual, bounds);
                }
            }
            (Node::Container(Container::Iterable, expected), _) => {
                if let Some(actual) = self.iterable_item(actual) {
                    self.constraints(expected[0], actual, variance, contextual, bounds);
                }
            }
            (Node::Container(kind @ (Container::Set | Container::FrozenSet), expected), _)
                if contextual
                    && self.class_id(actual).is_some_and(|id| {
                        self.class(id).namespace == "typing.AbstractSet"
                            || (*kind == Container::Set
                                && self.class(id).namespace == "typing.MutableSet")
                    }) =>
            {
                if let Some(item) = self.abstract_set_item(actual) {
                    self.constraints(expected[0], item, variance, contextual, bounds);
                }
            }
            (Node::Container(Container::Mapping, expected), _) => {
                if let Some((key, value)) = self.mapping_types(actual) {
                    self.constraints(expected[0], key, Variance::Invariant, contextual, bounds);
                    self.constraints(expected[1], value, variance, contextual, bounds);
                }
            }
            (
                Node::Container(
                    kind @ (Container::Awaitable
                    | Container::Coroutine
                    | Container::Generator
                    | Container::Iterator
                    | Container::AsyncGenerator
                    | Container::AsyncIterator
                    | Container::AsyncIterable),
                    expected,
                ),
                _,
            ) => {
                self.asynchronous_constraints(*kind, expected, actual, variance, contextual, bounds)
            }
            (Node::Container(kind, expected), Some(Node::Container(actual_kind, actual)))
                if kind == actual_kind =>
            {
                for (&expected, &actual) in expected.iter().zip(actual) {
                    let direction = if *kind == Container::FrozenSet {
                        variance
                    } else {
                        Variance::Invariant
                    };
                    self.constraints(expected, actual, direction, contextual, bounds);
                }
            }
            (Node::Tuple(expected), Some(Node::Tuple(actual))) => {
                if self.tuple_pack_constraints(expected, actual, variance, contextual, bounds) {
                    return;
                }
                for (&expected, &actual) in expected.iter().zip(actual) {
                    self.constraints(expected, actual, variance, contextual, bounds);
                }
            }
            (Node::Tuple(expected), Some(Node::VariadicTuple(item))) => {
                if let Some((prefix, target)) = self.tuple_pack_position(expected) {
                    self.constraints(target, actual, variance, contextual, bounds);
                    for &expected in expected[..prefix].iter().chain(&expected[prefix + 1..]) {
                        self.constraints(expected, *item, variance, contextual, bounds);
                    }
                }
            }
            (
                Node::AppliedClass {
                    id,
                    arguments: expected,
                    value,
                },
                _,
            ) if self.is_class_value(actual) == *value => {
                if let Some(actual) = self.class_arguments_for(actual, *id) {
                    for (index, (&expected, actual)) in expected.iter().zip(actual).enumerate() {
                        let variance = match self.class_parameter_variance(*id, index) {
                            ParameterVariance::Covariant => variance,
                            ParameterVariance::Contravariant => variance.reverse(),
                            ParameterVariance::Invariant | ParameterVariance::Inferred => {
                                Variance::Invariant
                            }
                        };
                        self.constraints(expected, actual, variance, contextual, bounds);
                    }
                }
            }
            (Node::VariadicTuple(expected), Some(Node::Tuple(actual))) => {
                for &actual in actual {
                    self.constraints(*expected, actual, variance, contextual, bounds);
                }
            }
            (Node::VariadicTuple(expected), Some(Node::VariadicTuple(actual))) => {
                self.constraints(*expected, *actual, variance, contextual, bounds)
            }
            _ => {}
        }
    }
}
