use mamba_language::application::quality_syntax::python_ast::{Expr, ExprCall, Number, Operator};
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    rc::Rc,
    sync::{Arc, Mutex},
};
mod abstract_members;
mod abstract_sets;
mod aggregation;
mod aggregation_containers;
mod aggregation_members;
mod aggregation_returns;
mod alias_expansion;
mod alias_inference;
mod alias_recursion;
mod alias_reveals;
pub(super) mod aliases;
mod annotations;
mod any_origin;
mod ast;
mod async_annotations;
mod async_iteration;
mod async_types;
mod builtin_alias_constructors;
mod builtin_class_aliases;
mod builtin_class_members;
mod builtin_constructors;
mod builtin_members;
mod builtin_protocols;
mod builtin_symbols;
mod builtin_values;
mod callable_attributes;
mod callables;
mod returned_callable_binders;
pub(in crate::infrastructure::mypy) use returned_callable_binders::{
    BorrowedSourceGuard as BorrowedReturnedCallableSourceGuard, Input as ReturnedCallableInput,
    SourceGuard as ReturnedCallableSourceGuard,
};
mod class_generics;
mod class_reveals;
mod class_values;
mod class_variance;
mod collection_constructors;
mod collection_functions;
mod collection_joins;
mod collection_methods;
pub(super) mod comparisons;
mod conditional;
pub(super) mod constraints;
mod constructor_declarations;
mod constructor_inputs;
mod constructors;
mod dataclass_replace;
mod descriptors;
mod dict_methods;
mod dictionary_inputs;
mod display;
mod flow;
mod function_variables;
mod generators;
mod generic_arguments;
mod graph;
mod higher_order;
mod intersections;
mod iterable_hints;
mod iteration_inputs;
mod list_union_index;
mod literals;
mod method_bindings;
mod module_globals;
mod nominal;
mod object_comparisons;
pub(super) mod operators;
mod overloads;
mod parameter_bounds;
mod parameter_specs;
mod predicates;
mod protocol_members;
mod protocols;
mod reversed_inference;
mod reversed_inputs;
mod runtime_narrowing;
mod sequence_comparisons;
mod sequence_inputs;
mod string_methods;
mod system_versions;
mod template_strings;
mod truthiness;
mod tuple_relations;
mod type_objects;
mod type_var_tuples;
mod typed_dict;
mod variables;
pub(in crate::infrastructure::mypy) use async_iteration::AsyncIterationError;
pub(crate) use graph::Context;
pub(super) use graph::{Container, Node};
pub(super) use literals::Literal;
pub(in crate::infrastructure::mypy) use variables::ParameterVariance;
pub(super) use variables::{VariableMap, Variables};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Type {
    Int,
    Float,
    Complex,
    Str,
    Bytes,
    Bool,
    None,
    Any,
    /// Any created only by runtime generic erasure; not a real Any for overload ambiguity.
    AnySpecial,
    Object,
    Never,
    Node(u32),
}

impl Type {
    /// Both atoms have ordinary Any compatibility and display semantics.
    pub(crate) fn is_any(self) -> bool {
        matches!(self, Self::Any | Self::AnySpecial)
    }

    /// Only ordinary Any triggers the existing overload ambiguity policy.
    pub(crate) fn is_real_any(self) -> bool {
        self == Self::Any
    }

    /// A scalar join retains its first Any operand, as in join_types.
    pub(crate) fn merge_any(self, other: Self) -> Self {
        debug_assert!(self.is_any() || other.is_any());
        if self.is_any() {
            self
        } else {
            other
        }
    }

    /// Operations on an Any value derive a new ordinary Any, unlike projection.
    pub(crate) fn derived_any(self) -> Self {
        if self.is_any() {
            Self::Any
        } else {
            self
        }
    }

    fn accepts_scalar(self, other: Self) -> bool {
        self == other
            || self.is_any()
            || other.is_any()
            || matches!(
                (self, other),
                (Self::Int, Self::Bool)
                    | (Self::Float, Self::Int | Self::Bool)
                    | (Self::Complex, Self::Int | Self::Float | Self::Bool)
            )
    }
}

pub(super) type ObservedExpressionTypes = Arc<Mutex<HashMap<usize, Option<Type>>>>;

struct ObservedExpressionGuard<'a> {
    slot: &'a RefCell<Option<ObservedExpressionTypes>>,
    previous: Option<ObservedExpressionTypes>,
}

impl Drop for ObservedExpressionGuard<'_> {
    fn drop(&mut self) {
        self.slot.replace(self.previous.take());
    }
}

// Own the Context, not a Scopes/Checker borrow. The copied call owns this guard.
pub(crate) struct ObservedCallGuard {
    context: Rc<Context>,
    previous: Option<ObservedExpressionTypes>,
}

impl Drop for ObservedCallGuard {
    fn drop(&mut self) {
        self.context
            .observed_expression_types
            .replace(self.previous.take());
    }
}

// Only literal True filters are covered by the frozen filtered SetComp evidence.
// Their type is bool and they do not change bindings or require narrowing.
pub(super) fn supported_set_comprehension_generators(
    generators: &[mamba_language::application::quality_syntax::python_ast::Comprehension],
) -> bool {
    generators.iter().all(|generator| {
        !generator.is_async
            && generator
                .ifs
                .iter()
                .all(|condition| matches!(condition, Expr::BooleanLiteral(value) if value.value))
    })
}

impl Context {
    pub(in crate::infrastructure::mypy) fn candidate_iterable_item(
        &self,
        ty: Type,
    ) -> Option<Type> {
        match ty {
            Type::Any => return Some(Type::Any),
            Type::Str => return Some(Type::Str),
            Type::Bytes => return Some(Type::Int),
            _ => {}
        }
        let node = self.node(ty)?;
        match node.as_ref() {
            Node::Tuple(items) => {
                let first = *items.first()?;
                items.iter().all(|&item| item == first).then_some(first)
            }
            Node::VariadicTuple(item) => Some(*item),
            Node::Container(kind, arguments) => match (kind, arguments.as_slice()) {
                (
                    Container::List
                    | Container::Sequence
                    | Container::Iterable
                    | Container::Iterator
                    | Container::Set
                    | Container::FrozenSet
                    | Container::DictKeys,
                    [item],
                ) => Some(*item),
                (Container::Generator, [item, _, _]) => Some(*item),
                (Container::Dict | Container::Mapping, [key, _]) => Some(*key),
                (Container::DictValues, [_, value]) => Some(*value),
                (Container::DictItems, [key, value]) => {
                    Some(self.intern(Node::Tuple(vec![*key, *value])))
                }
                _ => None,
            },
            _ => None,
        }
    }

    // Nominal iteration uses the same real protocol bindings as normal inference.
    pub(in crate::infrastructure::mypy) fn checked_candidate_iterable_item(
        &self,
        ty: Type,
        target: mamba_language::application::quality_syntax::PythonTarget,
    ) -> Option<Type> {
        if let Some(item) = self.candidate_iterable_item(ty) {
            return Some(item);
        }
        if !matches!(
            self.node(ty).as_deref(),
            Some(Node::Instance(_) | Node::AppliedClass { value: false, .. })
        ) || !self.candidate_value_role(ty, 0)
        {
            return None;
        }
        let returned =
            |(signature, error): (std::sync::Arc<super::signatures::Signature>, Option<String>)| {
                if error.is_some()
                    || signature.implicit
                    || signature.constructor
                    || signature.ignored
                    || signature.ellipsis
                    || signature.narrowing.is_some()
                    || !signature.parameters.is_empty()
                    || !signature.generic.is_empty()
                    || !signature.overloads.is_empty()
                    || signature.owner.is_none()
                    || signature.bound_receiver.is_none()
                {
                    return None;
                }
                signature.returns
            };
        let iterator = returned(self.class_method_call_binding_for(ty, "__iter__", target)?)?;
        if let Some(item) = self.candidate_iterable_item(iterator) {
            return self.candidate_value_role(item, 0).then_some(item);
        }
        if !matches!(
            self.node(iterator).as_deref(),
            Some(Node::Instance(_) | Node::AppliedClass { value: false, .. })
        ) || !self.candidate_value_role(iterator, 0)
        {
            return None;
        }
        let item = returned(self.class_method_call_binding_for(iterator, "__next__", target)?)?;
        self.candidate_value_role(item, 0).then_some(item)
    }

    // This reads represented value structure only. It never expands an alias,
    // loads a class, follows a variable bound or recovers a missing type as Any.
    pub(in crate::infrastructure::mypy) fn candidate_value_role(
        &self,
        ty: Type,
        depth: usize,
    ) -> bool {
        if depth >= 16 {
            return false;
        }
        match ty {
            Type::Bool
            | Type::Int
            | Type::Float
            | Type::Complex
            | Type::Str
            | Type::Bytes
            | Type::None
            | Type::Object
            | Type::Any => true,
            Type::Node(_) => match self.node(ty).as_deref() {
                Some(Node::Instance(_) | Node::Literal(_) | Node::LiteralValue(_)) => true,
                Some(Node::Container(_, items) | Node::Tuple(items) | Node::Union(items)) => items
                    .iter()
                    .all(|&item| self.candidate_value_role(item, depth + 1)),
                Some(Node::VariadicTuple(item)) => self.candidate_value_role(*item, depth + 1),
                Some(Node::AppliedClass {
                    arguments,
                    value: false,
                    ..
                }) => arguments
                    .iter()
                    .all(|&item| self.candidate_value_role(item, depth + 1)),
                _ => false,
            },
            _ => false,
        }
    }

    pub(in crate::infrastructure::mypy) fn has_observed_expression_types(&self) -> bool {
        self.observed_expression_types.borrow().is_some()
    }

    pub(in crate::infrastructure::mypy) fn with_checked_expression_types<T>(
        &self,
        observed: &ObservedExpressionTypes,
        check: impl FnOnce() -> T,
    ) -> T {
        let _guard = ObservedExpressionGuard {
            slot: &self.observed_expression_types,
            previous: self
                .observed_expression_types
                .replace(Some(observed.clone())),
        };
        check()
    }

    // A contextual lambda body has a fresh local environment, not checked inputs.
    pub(crate) fn without_observed<T>(&self, infer: impl FnOnce() -> T) -> T {
        let active = self.observed_expression_types.borrow().is_some();
        if !active {
            return infer();
        }
        let _guard = ObservedExpressionGuard {
            slot: &self.observed_expression_types,
            previous: self.observed_expression_types.replace(None),
        };
        infer()
    }

    // Transfer only checked values from this expansion's explicit original arguments.
    // No range/structural match, recomputation or persistent copied-address alias.
    pub(crate) fn observe_call_expansion(
        self: &Rc<Self>,
        original: &ExprCall,
        arguments: &[&Expr],
        expanded: &ExprCall,
    ) -> Option<ObservedCallGuard> {
        let previous = self.observed_expression_types.borrow().clone()?;
        let checked = previous.lock().expect("observed expression types").clone();
        let mut copied = checked.clone();
        let mut transfer = |original: &Expr, copy: &Expr| {
            if let Some(&ty) = checked.get(&(original as *const Expr as usize)) {
                copied.insert(copy as *const Expr as usize, ty);
            }
        };
        transfer(&original.func, &expanded.func);
        for (original, copy) in arguments.iter().zip(expanded.arguments.args.iter()) {
            transfer(original, copy);
        }
        for (original, copy) in original
            .arguments
            .keywords
            .iter()
            .zip(expanded.arguments.keywords.iter())
        {
            transfer(&original.value, &copy.value);
        }
        // All target Box/argument allocations are final and remain owned by ExpandedCall.
        Some(ObservedCallGuard {
            context: self.clone(),
            previous: self
                .observed_expression_types
                .replace(Some(Arc::new(Mutex::new(copied)))),
        })
    }

    // Keys refer to the original live AST. A checked unknown is not a cache miss.
    pub(crate) fn infer_with_observed(
        &self,
        expression: &Expr,
        environment: &BTreeMap<String, Type>,
        call_return: &impl Fn(&Expr) -> Option<Type>,
        observed: &ObservedExpressionTypes,
    ) -> Option<Type> {
        let _guard = ObservedExpressionGuard {
            slot: &self.observed_expression_types,
            previous: self
                .observed_expression_types
                .replace(Some(observed.clone())),
        };
        self.infer_with_calls(expression, environment, call_return)
    }

    // A checked unknown is a receipt, not an unhandled expression.
    pub(super) fn observed_expression_type(&self, expression: &Expr) -> Option<Option<Type>> {
        self.observed_expression_types
            .borrow()
            .as_ref()
            .and_then(|values| {
                let observed = values
                    .lock()
                    .expect("observed expression types")
                    .get(&(expression as *const Expr as usize))
                    .copied();
                observed
            })
    }

    pub(crate) fn infer_with_calls(
        &self,
        expression: &Expr,
        environment: &BTreeMap<String, Type>,
        call_return: &impl Fn(&Expr) -> Option<Type>,
    ) -> Option<Type> {
        if let Some(ty) = self.observed_expression_type(expression) {
            return ty;
        }
        let refinement = match expression {
            Expr::Attribute(_) | Expr::Subscript(_) => {
                crate::infrastructure::mypy::scopes::Scopes::attribute_key(expression)
            }
            _ => None,
        };
        if let Some(ty) = refinement.and_then(|key| environment.get(&key)).copied() {
            return Some(self.alias_runtime_value(ty));
        }
        match expression {
            Expr::BoolOp(_) | Expr::If(_) | Expr::Named(_) => {
                self.infer_conditional(expression, environment, call_return)
            }
            Expr::Name(name) => environment
                .get(name.id.as_str())
                .copied()
                .map(|ty| self.alias_runtime_value(ty))
                .or_else(|| call_return(expression)),
            Expr::StringLiteral(_) | Expr::FString(_) => Some(Type::Str),
            Expr::TString(_) => call_return(expression),
            Expr::BytesLiteral(_) => Some(Type::Bytes),
            Expr::BooleanLiteral(_) => Some(Type::Bool),
            Expr::NoneLiteral(_) => Some(Type::None),
            Expr::EllipsisLiteral(_) => {
                let id = self.declare_class("types.EllipsisType", "EllipsisType", 0);
                Some(self.class(id).instance)
            }
            Expr::NumberLiteral(number) => Some(match number.value {
                Number::Int(_) => Type::Int,
                Number::Float(_) => Type::Float,
                Number::Complex { .. } => Type::Complex,
            }),
            Expr::UnaryOp(unary)
                if matches!(
                    unary.op,
                    mamba_language::application::quality_syntax::python_ast::UnaryOp::USub
                        | mamba_language::application::quality_syntax::python_ast::UnaryOp::UAdd
                ) =>
            {
                self.infer_with_calls(&unary.operand, environment, call_return)
                    .map(Type::derived_any)
            }
            Expr::BinOp(binary) => {
                let left = self.infer_with_calls(&binary.left, environment, call_return)?;
                let right = self.infer_with_calls(&binary.right, environment, call_return)?;
                if left.is_any() || right.is_any() {
                    return Some(Type::Any);
                }
                if binary.op == Operator::BitOr {
                    if let Some(result) = self.runtime_class_union(left, right) {
                        return Some(result);
                    }
                    if let Some(result) =
                        self.dictionary_or_expression(binary, left, right, environment, call_return)
                    {
                        return Some(result);
                    }
                }
                Self::primitive_binary_result(left, right, binary.op)
                    .or_else(|| self.binary_result(left, right, binary.op))
            }
            Expr::Attribute(_) => call_return(expression).map(|ty| self.alias_runtime_value(ty)),
            Expr::Yield(_) | Expr::YieldFrom(_) => call_return(expression),
            Expr::Generator(_) | Expr::ListComp(_) | Expr::DictComp(_) => call_return(expression),
            Expr::SetComp(set) if supported_set_comprehension_generators(&set.generators) => {
                call_return(expression)
            }
            Expr::Lambda(_) => call_return(expression),
            Expr::Await(awaited) => Some(
                self.await_result(self.infer_with_calls(
                    &awaited.value,
                    environment,
                    call_return,
                )?)
                .unwrap_or(Type::Any),
            ),
            Expr::Compare(_) => call_return(expression),
            Expr::Call(call) => {
                if matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "reveal_type")
                {
                    call_return(expression)
                } else if self
                    .infer_with_calls(&call.func, environment, call_return)
                    .is_some_and(Type::is_any)
                {
                    Some(Type::Any)
                } else {
                    call_return(expression)
                }
            }
            Expr::List(list) => {
                self.infer_items(graph::Container::List, &list.elts, environment, call_return)
            }
            Expr::Set(set) => {
                self.infer_items(graph::Container::Set, &set.elts, environment, call_return)
            }
            Expr::Tuple(tuple) => Some(
                self.intern(graph::Node::Tuple(
                    tuple
                        .elts
                        .iter()
                        .map(|value| self.infer_with_calls(value, environment, call_return))
                        .collect::<Option<Vec<_>>>()?,
                )),
            ),
            Expr::Dict(dictionary) => {
                if let Some(returned) = call_return(expression) {
                    return Some(returned);
                }
                let mut keys = None;
                let mut values = None;
                for item in &dictionary.items {
                    let key =
                        self.infer_with_calls(item.key.as_ref()?, environment, call_return)?;
                    let value = self.infer_with_calls(&item.value, environment, call_return)?;
                    keys = Some(keys.map_or(key, |previous| self.join(previous, key)));
                    values = Some(values.map_or(value, |previous| self.join(previous, value)));
                }
                Some(self.intern(graph::Node::Container(
                    graph::Container::Dict,
                    vec![keys.unwrap_or(Type::Any), values.unwrap_or(Type::Any)],
                )))
            }
            Expr::Subscript(subscript) => {
                if let Some(returned) = call_return(expression) {
                    return Some(returned);
                }
                let value = self.infer_with_calls(&subscript.value, environment, call_return)?;
                if value.is_any() {
                    return Some(Type::Any);
                }
                if self.alias_id(value).is_some() {
                    if self.alias(self.alias_id(value).unwrap()).modern {
                        return Some(value);
                    }
                    let expanded = self.annotation_with_variables(expression, environment)?;
                    return Some(self.class_value_type(expanded));
                }
                if self.is_class_value(value) {
                    if self.invalid_class_index(value) {
                        return Some(Type::Any);
                    }
                    let mut variables = environment.clone();
                    if let Some(name) = annotations::annotation_name(&subscript.value) {
                        let marker = if self
                            .class_id(value)
                            .is_some_and(|id| self.class(id).namespace == "builtins.type")
                        {
                            self.annotation_value(value)
                        } else {
                            value
                        };
                        variables.entry(name).or_insert(marker);
                    }
                    let annotation = self.annotation_with_variables(expression, &variables)?;
                    return Some(self.class_value_type(annotation));
                }
                if let Some(items) = self.system_version_items(value) {
                    return self.system_version_index(&items, &subscript.slice);
                }
                if subscript.ctx.is_load() && value == Type::Str {
                    // The result stays str even when the index check reports an error.
                    return Some(Type::Str);
                }
                let node = self.node(value)?;
                match node.as_ref() {
                    graph::Node::Union(items) if subscript.ctx.is_load() => self
                        .list_union_index_result(items, subscript.slice.is_slice_expr())
                        .or_else(|| {
                            self.optional_dict_index_result(
                                items,
                                subscript.slice.is_slice_expr(),
                                || {
                                    self.infer_with_calls(
                                        &subscript.slice,
                                        environment,
                                        call_return,
                                    )
                                },
                            )
                        }),
                    graph::Node::Container(graph::Container::List, _)
                        if matches!(subscript.slice.as_ref(), Expr::Slice(_)) =>
                    {
                        Some(value)
                    }
                    graph::Node::Container(
                        graph::Container::List | graph::Container::Sequence,
                        args,
                    ) => Some(args[0]),
                    graph::Node::Container(graph::Container::Dict, args) => Some(args[1]),
                    graph::Node::VariadicTuple(item) => Some(*item),
                    graph::Node::Tuple(items) => {
                        let Expr::NumberLiteral(number) = subscript.slice.as_ref() else {
                            return None;
                        };
                        let Number::Int(index) = &number.value else {
                            return None;
                        };
                        items.get(index.as_usize()?).copied()
                    }
                    graph::Node::Instance(_) if self.typed_dict(value).is_some() => {
                        let (_, definition) = self.typed_dict(value)?;
                        let keys = self.typed_dict_keys(&subscript.slice, environment);
                        let Some(keys) = keys else {
                            return Some(Type::Any);
                        };
                        let values = keys
                            .iter()
                            .map(|key| {
                                definition
                                    .items
                                    .iter()
                                    .find(|item| &item.name == key)
                                    .map(|item| item.ty)
                            })
                            .collect::<Option<Vec<_>>>();
                        Some(values.map(|values| self.union(values)).unwrap_or(Type::Any))
                    }
                    _ => None,
                }
            }
            Expr::Slice(_) => call_return(expression),
            _ => None,
        }
    }

    pub(in crate::infrastructure::mypy) fn infer_partial_set_item(
        &self,
        expression: &Expr,
        environment: &BTreeMap<String, Type>,
        call_return: &impl Fn(&Expr) -> Option<Type>,
    ) -> Option<Type> {
        let Expr::Tuple(tuple) = expression else {
            return self.infer_with_calls(expression, environment, call_return);
        };
        if !tuple.ctx.is_load() {
            return self.infer_with_calls(expression, environment, call_return);
        }
        if let Some(observed) = self.observed_expression_type(expression) {
            return observed;
        }
        // This entry is owned by partial-set item inference with no expected type.
        // Other expression kinds retain the original refinement and call rules.
        if tuple.elts.iter().any(Expr::is_starred_expr) {
            self.infer_homogeneous_starred_tuple(&tuple.elts, environment, call_return)
        } else {
            Some(
                self.intern(graph::Node::Tuple(
                    tuple
                        .elts
                        .iter()
                        .map(|value| self.infer_partial_set_item(value, environment, call_return))
                        .collect::<Option<Vec<_>>>()?,
                )),
            )
        }
    }

    fn infer_homogeneous_starred_tuple(
        &self,
        items: &[Expr],
        environment: &BTreeMap<String, Type>,
        call_return: &impl Fn(&Expr) -> Option<Type>,
    ) -> Option<Type> {
        let mut common: Option<Type> = None;
        let mut has_starred = false;
        for value in items {
            let item = if let Expr::Starred(starred) = value {
                if !starred.ctx.is_load() {
                    return None;
                }
                has_starred = true;
                let operand = self.infer_with_calls(&starred.value, environment, call_return)?;
                let node = self.node(operand)?;
                match node.as_ref() {
                    graph::Node::VariadicTuple(item) => *item,
                    graph::Node::Container(graph::Container::List, items) if items.len() == 1 => {
                        items[0]
                    }
                    _ => return None,
                }
            } else {
                self.infer_with_calls(value, environment, call_return)?
            };
            if !matches!(
                item,
                Type::Bool
                    | Type::Int
                    | Type::Float
                    | Type::Complex
                    | Type::Str
                    | Type::Bytes
                    | Type::None
                    | Type::Object
            ) || common.is_some_and(|previous| previous != item)
            {
                return None;
            }
            common = Some(item);
        }
        if !has_starred {
            return None;
        }
        Some(self.intern(graph::Node::VariadicTuple(common?)))
    }

    // Only a proved fresh local assignment owns ambiguous-bottom recovery.
    // Ordinary callers keep infer_items, including its starred-input policy.
    pub(in crate::infrastructure::mypy) fn infer_list_common_assignment(
        &self,
        expression: &Expr,
        environment: &BTreeMap<String, Type>,
        call_return: &impl Fn(&Expr) -> Option<Type>,
    ) -> Option<Type> {
        let Expr::List(list) = expression else {
            return self.infer_with_calls(expression, environment, call_return);
        };
        if self.observed_expression_type(expression).is_some()
            || !self.list_common_sources(&list.elts, environment)
        {
            return self.infer_with_calls(expression, environment, call_return);
        }
        // The source proof excludes stars. Preserve the old nonstarred item
        // inference and fold order, and retain those exact results for solving.
        let mut item_type = None;
        let mut bounds = Vec::new();
        for item in &list.elts {
            let ty = self.infer_with_calls(item, environment, call_return)?;
            item_type = Some(item_type.map_or(ty, |previous| self.join(previous, ty)));
            bounds.push(ty);
        }
        self.literal_list_common_type(&list.elts, &bounds, environment)
            .or_else(|| {
                Some(self.intern(graph::Node::Container(
                    graph::Container::List,
                    vec![item_type.unwrap_or(Type::Any)],
                )))
            })
    }

    fn infer_items(
        &self,
        kind: graph::Container,
        items: &[Expr],
        environment: &BTreeMap<String, Type>,
        call_return: &impl Fn(&Expr) -> Option<Type>,
    ) -> Option<Type> {
        let mut item_type = None;
        for item in items {
            let ty = if let Expr::Starred(starred) = item {
                // Direct empty syntax has no represented item bound here.
                // Keep its old unknown path instead of turning that bound into Any.
                if matches!(starred.value.as_ref(), Expr::List(value) if value.elts.is_empty())
                    || matches!(starred.value.as_ref(), Expr::Tuple(value) if value.elts.is_empty())
                    || matches!(starred.value.as_ref(), Expr::Set(value) if value.elts.is_empty())
                    || matches!(starred.value.as_ref(), Expr::Dict(value) if value.items.is_empty())
                {
                    return None;
                }
                let iterable = self.infer_with_calls(&starred.value, environment, call_return)?;
                let item = self.iterable_item(iterable)?;
                // Fixed heterogeneous tuples need individual argument expansion,
                // rather than the iterable union used by an ordinary loop.
                if item == Type::Never
                    || (matches!(self.node(iterable).as_deref(), Some(graph::Node::Tuple(_)))
                        && matches!(self.node(item).as_deref(), Some(graph::Node::Union(_))))
                {
                    return None;
                }
                item
            } else {
                self.infer_with_calls(item, environment, call_return)?
            };
            item_type = Some(item_type.map_or(ty, |previous| self.join(previous, ty)));
        }
        Some(self.intern(graph::Node::Container(
            kind,
            vec![item_type.unwrap_or(Type::Any)],
        )))
    }
}

#[cfg(test)]
mod ast_tests;
#[cfg(test)]
mod fstring_observation_tests;
