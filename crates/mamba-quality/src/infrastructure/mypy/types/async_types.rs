//! Coroutine results and awaitable protocols share the ordinary type graph.
use super::super::{
    classes::{Abstractness, Method, MethodKind},
    signatures::{Kind, Parameter, Signature},
};
use super::*;
use std::{collections::BTreeSet, sync::Arc};

impl Context {
    pub(in crate::infrastructure::mypy) fn coroutine(&self, result: Type) -> Type {
        self.intern(Node::Container(
            Container::Coroutine,
            vec![Type::Any, Type::Any, result],
        ))
    }

    pub(in crate::infrastructure::mypy) fn coroutine_result(&self, ty: Type) -> Option<Type> {
        match self.node(ty).as_deref() {
            Some(Node::Container(Container::Coroutine, arguments)) => Some(arguments[2]),
            _ => None,
        }
    }

    pub(in crate::infrastructure::mypy) fn await_result(&self, ty: Type) -> Option<Type> {
        self.await_result_inner(ty, &mut BTreeSet::new())
    }

    fn await_result_inner(&self, ty: Type, active: &mut BTreeSet<Type>) -> Option<Type> {
        if ty.is_any() {
            return Some(Type::Any);
        }
        if !active.insert(ty) {
            return None;
        }
        let expanded = self.expanded_alias_instance(ty);
        let result = if expanded != ty {
            self.await_result_inner(expanded, active)
        } else {
            match self.node(ty).as_deref() {
                Some(Node::Container(Container::Awaitable, arguments)) => Some(arguments[0]),
                Some(Node::Container(Container::Coroutine, arguments)) => Some(arguments[2]),
                Some(Node::Variable(variable)) => self.await_result_inner(variable.bound, active),
                Some(Node::Union(items)) => items
                    .iter()
                    .map(|&item| self.await_result_inner(item, active))
                    .collect::<Option<Vec<_>>>()
                    .map(|items| self.union(items)),
                _ if self.class_id(ty).is_some() && !self.is_class_value(ty) => {
                    if !self.protocol_accepts(self.awaitable_protocol(), ty) {
                        None
                    } else {
                        self.class_method(ty, "__await__")
                            .and_then(|signature| signature.returns)
                            .and_then(|returns| {
                                if returns.is_any() {
                                    Some(Type::Any)
                                } else if let Some(Node::Container(
                                    Container::Generator,
                                    arguments,
                                )) = self.node(returns).as_deref()
                                {
                                    Some(arguments[2])
                                } else {
                                    None
                                }
                            })
                    }
                }
                _ => None,
            }
        };
        active.remove(&ty);
        result
    }

    pub(in crate::infrastructure::mypy) fn awaitable_protocol(&self) -> Type {
        let id = self.declare_class("typing", "Awaitable", 122);
        let old = self.class(id);
        if old.protocol {
            return old.instance;
        }
        let mut class = (*old).clone();
        class.protocol = true;
        let returns = self.intern(Node::Container(Container::Generator, vec![Type::Any; 3]));
        let mut signature = Signature::empty("__await__".into(), returns);
        signature.constructor = false;
        signature.parameters = vec![Parameter {
            name: "self".into(),
            ty: Some(class.instance),
            kind: Kind::PositionalOrKeyword,
            required: true,
        }]
        .into();
        class.methods.insert(
            "__await__".into(),
            Method {
                signature: Arc::new(signature),
                kind: MethodKind::Instance,
                setter: None,
                final_method: false,
                abstractness: Abstractness::Explicit,
            },
        );
        let result = class.instance;
        self.replace_class(id, class);
        result
    }

    pub(super) fn asynchronous_accepts(
        &self,
        kind: Container,
        args: &[Type],
        actual: Type,
    ) -> bool {
        if kind == Container::Awaitable {
            return self
                .await_result(actual)
                .is_some_and(|result| self.accepts(args[0], result));
        }
        if kind == Container::Iterator && self.class_id(actual).is_some() {
            return self
                .iterator_item(actual)
                .is_some_and(|item| self.accepts(args[0], item));
        }
        let actual_node = self.node(actual);
        let Some(Node::Container(other, values)) = actual_node.as_deref() else {
            return false;
        };
        if kind == Container::Iterator {
            return matches!(other, Container::Iterator | Container::Generator)
                && self.accepts(args[0], values[0]);
        }
        if matches!(kind, Container::AsyncIterator | Container::AsyncIterable) {
            return (matches!(other, Container::AsyncIterator | Container::AsyncGenerator)
                || (kind == Container::AsyncIterable && *other == Container::AsyncIterable))
                && self.accepts(args[0], values[0]);
        }
        kind == *other
            && args.len() == values.len()
            && args
                .iter()
                .zip(values)
                .enumerate()
                .all(|(index, (&expected, &actual))| {
                    if index == 1 {
                        self.accepts(actual, expected)
                    } else {
                        self.accepts(expected, actual)
                    }
                })
    }
}
