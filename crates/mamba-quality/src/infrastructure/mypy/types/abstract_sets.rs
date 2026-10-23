//! Built-in containers retain their standard collection superclass relations.
use super::*;

impl Context {
    pub(super) fn collection_has_nominal_base(&self, actual: Type, owner: u32) -> bool {
        if self.class(owner).namespace == "builtins._SupportsReversed"
            && matches!(
                self.node(actual).as_deref(),
                Some(Node::Container(_, _) | Node::Tuple(_) | Node::VariadicTuple(_))
            )
            && self.class_method(actual, "__reversed__").is_some()
        {
            return true;
        }
        if self.class(owner).namespace == "_typeshed.SupportsIter"
            && self.class_id(actual).is_none()
            && self.iterable_item(actual).is_some()
        {
            return true;
        }
        if let Some(actual_id) = self.class_id(actual) {
            return self.nominal_accepts(owner, actual_id);
        }
        !matches!(actual, Type::Str | Type::Bytes)
            && self.builtin_collection_arguments(actual, owner).is_some()
    }

    pub(super) fn collection_conflict_name(&self, ty: Type) -> String {
        if matches!(self.node(ty).as_deref(), Some(Node::Tuple(_))) {
            let item = self.sequence_item(ty).unwrap_or(Type::Never);
            return self.name(self.intern(Node::VariadicTuple(item)));
        }
        self.name(ty)
    }
    pub(super) fn standard_iterable_form(&self, ty: Type) -> Option<Type> {
        let id = self.class_id(ty)?;
        (self.class(id).namespace == "typing.Iterable" && !self.is_class_value(ty)).then(|| {
            self.intern(Node::Container(
                Container::Iterable,
                self.class_arguments(ty),
            ))
        })
    }
    pub(super) fn builtin_abstract_set_accepts(
        &self,
        expected: Type,
        actual: Type,
    ) -> Option<bool> {
        let id = self.class_id(expected)?;
        if self.class(id).namespace == "typing.Sized" && self.typed_dict(actual).is_some() {
            return Some(!self.is_class_value(actual));
        }
        if self.class(id).namespace == "typing.Iterable" && self.typed_dict(actual).is_some() {
            return Some(
                !self.is_class_value(actual)
                    && self
                        .class_arguments(expected)
                        .first()
                        .is_none_or(|&item| self.accepts(item, Type::Str)),
            );
        }
        if self.is_class_value(expected) {
            return None;
        }
        if matches!(
            self.class(id).namespace.as_str(),
            "typing.Iterable" | "typing.Sequence" | "typing.Reversible"
        ) {
            let item = self
                .class_arguments(expected)
                .first()
                .copied()
                .unwrap_or(Type::Any);
            if let Some(accepted) = self.tuple_element_accepts(item, actual) {
                return Some(accepted);
            }
        }
        let arguments = self.builtin_collection_arguments(actual, id)?;
        let expected = self.class_arguments(expected);
        Some(
            expected
                .iter()
                .zip(arguments)
                .enumerate()
                .all(|(index, (&expected, actual))| {
                    self.accepts(expected, actual)
                        && (self.class_parameter_variance(id, index)
                            == ParameterVariance::Covariant
                            || self.accepts(actual, expected))
                }),
        )
    }

    pub(in crate::infrastructure::mypy) fn builtin_collection_arguments(
        &self,
        actual: Type,
        owner: u32,
    ) -> Option<Vec<Type>> {
        let class = self.class(owner);
        let source_name = self.embedded_source_class_name(owner);
        let namespace = source_name.as_deref().unwrap_or(class.namespace.as_str());
        if namespace == "_typeshed.SupportsKeysAndGetItem" {
            let (key, value) = self.mapping_types(actual)?;
            return Some(vec![key, value]);
        }
        if self.class_id(actual).is_some() {
            return None;
        }
        let item = match namespace {
            "_typeshed.SupportsLenAndGetItem" => {
                if matches!(self.node(actual).as_deref(), Some(Node::Tuple(items)) if items.is_empty())
                {
                    Type::Never
                } else {
                    self.sequence_item(actual)?
                }
            }
            "typing.Sized" => {
                if self.sequence_item(actual).is_some()
                    || matches!(
                        self.node(actual).as_deref(),
                        Some(
                            Node::Tuple(_)
                                | Node::VariadicTuple(_)
                                | Node::Container(
                                    Container::Set
                                        | Container::FrozenSet
                                        | Container::Dict
                                        | Container::Mapping
                                        | Container::DictKeys
                                        | Container::DictValues
                                        | Container::DictItems,
                                    _
                                )
                        )
                    )
                {
                    return Some(Vec::new());
                }
                return None;
            }
            "typing.Container" | "typing.Collection" => {
                if matches!(
                    self.node(actual).as_deref(),
                    Some(Node::Container(
                        Container::Iterable | Container::Iterator | Container::Generator,
                        _
                    ))
                ) {
                    return None;
                }
                self.iterable_item(actual)?
            }
            "typing.Reversible" => {
                if !matches!(actual, Type::Str | Type::Bytes)
                    && !matches!(
                        self.node(actual).as_deref(),
                        Some(
                            Node::Tuple(_)
                                | Node::VariadicTuple(_)
                                | Node::Container(
                                    Container::List
                                        | Container::Sequence
                                        | Container::Dict
                                        | Container::DictKeys
                                        | Container::DictValues
                                        | Container::DictItems,
                                    _
                                )
                        )
                    )
                {
                    return None;
                }
                self.iterable_item(actual)?
            }
            "typing.Iterable" => self.iterable_item(actual)?,
            "typing.Iterator" => self.iterator_item(actual)?,
            "typing.MutableSequence" => {
                // Only the real standard owner may give compact List this base.
                if source_name.is_none()
                    && self
                        .classes
                        .borrow()
                        .keys
                        .get(&(class.namespace.clone(), 0))
                        != Some(&owner)
                {
                    return None;
                }
                let [parameter] = class.generic.as_slice() else {
                    return None;
                };
                if !matches!(self.node(*parameter).as_deref(), Some(Node::Variable(variable))
                    if variable.variance == ParameterVariance::Invariant
                        && !variable.parameter_spec && !variable.type_var_tuple)
                {
                    return None;
                }
                let node = self.node(actual)?;
                let Node::Container(Container::List, arguments) = node.as_ref() else {
                    return None;
                };
                let [item] = arguments.as_slice() else {
                    return None;
                };
                *item
            }
            "typing.AbstractSet" | "typing.MutableSet" => {
                let node = self.node(actual)?;
                let Node::Container(kind, arguments) = node.as_ref() else {
                    return None;
                };
                if namespace == "typing.MutableSet" && *kind != Container::Set {
                    return None;
                }
                match kind {
                    Container::Set | Container::FrozenSet | Container::DictKeys => arguments[0],
                    Container::DictItems => self.normalize_tuple(arguments.clone()),
                    _ => return None,
                }
            }
            _ => return None,
        };
        Some(vec![item])
    }

    pub(in crate::infrastructure::mypy) fn abstract_set_item(&self, ty: Type) -> Option<Type> {
        let id = self.class_id(ty)?;
        if self.is_class_value(ty)
            || !matches!(
                self.class(id).namespace.as_str(),
                "typing.AbstractSet" | "typing.MutableSet"
            )
        {
            return None;
        }
        Some(
            self.class_arguments(ty)
                .first()
                .copied()
                .unwrap_or(Type::Any),
        )
    }

    pub(in crate::infrastructure::mypy) fn collection_literal_item(
        &self,
        ty: Type,
    ) -> Option<Type> {
        let id = self.class_id(ty)?;
        if self.class(id).namespace == "builtins._SupportsReversed" {
            let returned = self.class_arguments(ty).first().copied()?;
            let id = self.class_id(returned)?;
            if matches!(
                self.class(id).namespace.as_str(),
                "typing.Iterator" | "typing.Iterable"
            ) {
                return self.class_arguments(returned).first().copied();
            }
            return None;
        }
        if self.class(id).namespace == "_typeshed.SupportsIter" {
            let iterator = self.class_arguments(ty).first().copied()?;
            let id = self.class_id(iterator)?;
            if self.class(id).namespace == "typing.Iterator" {
                return self.class_arguments(iterator).first().copied();
            }
        }
        if self.is_class_value(ty)
            || !matches!(
                self.class(id).namespace.as_str(),
                "typing.AbstractSet"
                    | "typing.MutableSet"
                    | "typing.Container"
                    | "typing.Collection"
                    | "typing.Iterable"
                    | "typing.Reversible"
            )
        {
            return None;
        }
        self.class_arguments(ty).first().copied()
    }
}
