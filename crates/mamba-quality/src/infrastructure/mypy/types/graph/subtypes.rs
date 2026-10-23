//! Structural subtype checks retain scalar, callable, and nominal identities.
use super::*;

impl Context {
    // Compact Dict has no nominal base. Use the same canonical owner proof as
    // collection annotations, and keep both MutableMapping parameters invariant.
    fn compact_dict_mutable_mapping_accepts(&self, expected: Type, actual: Type) -> Option<bool> {
        let actual_node = self.node(actual)?;
        let Node::Container(Container::Dict, actual_arguments) = actual_node.as_ref() else {
            return None;
        };
        let expected_node = self.node(expected)?;
        let Node::AppliedClass {
            id,
            arguments,
            value: false,
        } = expected_node.as_ref()
        else {
            return None;
        };
        let class = self.class(*id);
        if actual_arguments.len() != 2 || arguments.len() != 2 || class.generic.len() != 2 {
            return None;
        }
        let fullname = self.embedded_source_class_name(*id).or_else(|| {
            (self
                .classes
                .borrow()
                .keys
                .get(&(class.namespace.clone(), 0))
                == Some(id))
            .then(|| class.namespace.clone())
        })?;
        if fullname != "typing.MutableMapping" {
            return None;
        }
        Some(
            arguments
                .iter()
                .zip(actual_arguments)
                .all(|(&expected, &actual)| {
                    self.accepts(expected, actual) && self.accepts(actual, expected)
                }),
        )
    }

    pub(crate) fn accepts(&self, expected: Type, actual: Type) -> bool {
        if self
            .node(actual)
            .is_some_and(|node| matches!(node.as_ref(), Node::Deleted(_)))
        {
            return true;
        }
        if self.is_alias_instance(expected) || self.is_alias_instance(actual) {
            return self.recursive_alias_accepts(expected, actual);
        }
        if expected == actual
            || expected.is_any()
            || actual.is_any()
            || expected == Type::Object
            || actual == Type::Never
        {
            return true;
        }
        // Compact dictionaries have no nominal class ID. Compare them through
        // the same proved canonical Mapping form used by value annotations.
        // Nominal actuals retain their existing inheritance/protocol route.
        if matches!(
            self.node(actual).as_deref(),
            Some(Node::Container(Container::Dict | Container::Mapping, _))
        ) && self.canonical_collection_kind(expected) == Some(Container::Mapping)
        {
            let compact = self.compact_applied_collection_annotation(expected);
            // Alias declarations keep their nominal owner while being built.
            if compact != expected {
                return self.accepts(compact, actual);
            }
        }
        if let Some(accepted) = self.compact_dict_mutable_mapping_accepts(expected, actual) {
            return accepted;
        }
        // Mypy's ordinary instance subtype query admits an Any-base fallback,
        // except for the non-subclassable None singleton. Proper queries differ.
        if self.instance_fallback_to_any(actual) {
            return expected != Type::None;
        }
        if let Some(Node::Union(items)) = self.node(actual).as_deref() {
            // Keep deleted expectations invalid; every actual member must fit.
            return !matches!(self.node(expected).as_deref(), Some(Node::Deleted(_)))
                && items.iter().all(|&actual| self.accepts(expected, actual));
        }
        if let Some(accepted) = self.builtin_abstract_set_accepts(expected, actual) {
            return accepted;
        }
        if let Some(node) = self.node(actual) {
            if let Node::Literal(value) | Node::LiteralValue(value) = node.as_ref() {
                if !self.contains_literal(expected) {
                    return self.accepts(expected, value.base());
                }
            }
            if let Node::Variable(variable) = node.as_ref() {
                return self.accepts(expected, variable.bound);
            }
        }
        if let Some(node) = self.node(expected) {
            if let Node::LiteralValue(value) = node.as_ref() {
                return self.accepts(value.base(), actual);
            }
        }
        match (self.node(expected), self.node(actual)) {
            (Some(expected), actual_node) => match expected.as_ref() {
                Node::Deleted(_) => false,
                Node::Union(items) => {
                    if let Some(actual) = actual_node {
                        if let Node::Union(actuals) = actual.as_ref() {
                            return actuals.iter().all(|&actual| {
                                items.iter().any(|&expected| self.accepts(expected, actual))
                            });
                        }
                    }
                    items.iter().any(|&expected| self.accepts(expected, actual))
                }
                Node::Container(kind @ (Container::Sequence | Container::Iterable), args) => {
                    self.iterable_input_accepts(*kind, args[0], actual)
                }
                Node::Container(Container::Mapping, args) => {
                    self.mapping_types(actual).is_some_and(|(key, value)| {
                        self.accepts(args[0], key)
                            && self.accepts(key, args[0])
                            && self.accepts(args[1], value)
                    })
                }
                Node::Container(Container::TypeObject, args) => {
                    (args[0].is_any()
                        && self
                            .class_id(actual)
                            .is_some_and(|id| self.class(id).namespace == "builtins.type"))
                        || self
                            .class_object_instance(args[0], actual)
                            .filter(|_| self.is_class_value(actual))
                            .is_some_and(|instance| self.accepts(args[0], instance))
                }
                Node::Container(
                    kind @ (Container::Awaitable
                    | Container::Coroutine
                    | Container::Generator
                    | Container::Iterator
                    | Container::AsyncGenerator
                    | Container::AsyncIterator
                    | Container::AsyncIterable),
                    args,
                ) => self.asynchronous_accepts(*kind, args, actual),
                Node::Container(Container::FrozenSet, args) => actual_node.is_some_and(|actual| {
                    matches!(actual.as_ref(), Node::Container(Container::FrozenSet, actual_args)
                        if self.accepts(args[0], actual_args[0]))
                }),
                Node::Container(Container::Dict, args) if self.class_id(actual).is_some() => {
                    let inherited = (|| {
                        let root = self.class_id(actual)?;
                        let namespace = self.class(root).namespace.clone();
                        let target = self.classes.borrow().module_targets.get(&namespace).copied()?;
                        self.nominal_dict_base(actual, target).map(|(_, items)| items)
                    })();
                    inherited.is_some_and(|items| {
                        args.len() == items.len()
                            && args.iter().zip(items).all(|(&expected, actual)| {
                                self.accepts(expected, actual) && self.accepts(actual, expected)
                            })
                    })
                }
                Node::Container(kind, args) => actual_node.is_some_and(|actual| {
                    matches!(actual.as_ref(), Node::Container(actual_kind, actual_args)
                        if kind == actual_kind && args.len() == actual_args.len()
                            && args.iter().zip(actual_args).all(|(&expected, &actual)|
                                self.accepts(expected, actual) && self.accepts(actual, expected)))
                }),
                Node::Tuple(args) => actual_node.is_some_and(|actual| {
                    matches!(actual.as_ref(), Node::Tuple(actual_args)
                        if self.tuple_pattern_accepts(args, actual_args))
                }),
                Node::VariadicTuple(item) => {
                    actual_node.is_some_and(|actual| match actual.as_ref() {
                        Node::Tuple(args) => args.iter().all(|&actual| self.accepts(*item, actual)),
                        Node::VariadicTuple(actual) => self.accepts(*item, *actual),
                        _ => false,
                    })
                }
                Node::Variable(_) => false,
                Node::Parameters(parameters, ellipsis) => {
                    actual_node.is_some_and(|actual| match actual.as_ref() {
                        Node::Parameters(actual, actual_ellipsis) => {
                            self.accepts_parameters(parameters, *ellipsis, actual, *actual_ellipsis)
                        }
                        _ => false,
                    })
                }
                Node::ParamSpecArgs(_) | Node::ParamSpecKwargs(_) | Node::Unpack(_) => false,
                Node::Literal(_)
                | Node::LiteralValue(_)
                | Node::LiteralForm
                | Node::TypingForm(_)
                | Node::Alias(_, _)
                | Node::AliasInstance(_, _)
                | Node::InferredAlias { .. } => false,
                Node::Instance(expected)
                    if self.class(*expected).namespace == "builtins.type"
                        && (self.node(actual).is_some_and(|node| matches!(node.as_ref(), Node::Union(items) if items.iter().all(|&item| self.accepts(self.class(*expected).instance, item))))
                            || self.is_class_value(actual)
                            || self
                                .callable_signature(actual)
                                .is_some_and(|signature| signature.constructor)) =>
                {
                    true
                }
                Node::Instance(expected) if self.class(*expected).protocol => {
                    self.protocol_accepts(self.class(*expected).instance, actual)
                }
                Node::Instance(expected) => self.class_id(actual).is_some_and(|actual_id| {
                    !self.is_class_value(actual) && self.nominal_accepts(*expected, actual_id)
                }),
                Node::Class(expected) if self.class(*expected).protocol => {
                    self.is_class_value(actual)
                        && self.instance_type(actual).is_some_and(|actual| {
                            self.protocol_accepts(self.class(*expected).instance, actual)
                        })
                }
                Node::Class(expected) => self.class_id(actual).is_some_and(|actual_id| {
                    self.is_class_value(actual) && self.nominal_accepts(*expected, actual_id)
                }),
                Node::AppliedClass {
                    id,
                    arguments,
                    value: false,
                } if self.class(*id).protocol => {
                    self.protocol_accepts(self.applied_class(*id, arguments.clone(), false), actual)
                }
                Node::AppliedClass {
                    id,
                    arguments,
                    value,
                } => self.applied_class_accepts(*id, arguments, *value, actual),
                Node::Callable(expected) => self
                    .callable_signature(actual)
                    .is_some_and(|actual| self.accepts_callable(expected, &actual, false)),
            },
            (None, Some(actual)) => match actual.as_ref() {
                Node::Union(items) => items.iter().all(|&actual| self.accepts(expected, actual)),
                Node::Variable(variable) => self.accepts(expected, variable.bound),
                Node::Instance(id) => self.scalar_base_accepts(expected, *id),
                Node::AppliedClass {
                    id, value: false, ..
                } => self.scalar_base_accepts(expected, *id),
                _ => false,
            },
            (None, None) => expected.accepts_scalar(actual),
        }
    }
}
