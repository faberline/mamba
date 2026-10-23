//! Runtime type checks retain exact class targets and possible subclass bounds.
use super::*;

impl Context {
    pub(in crate::infrastructure::mypy) fn runtime_type_ranges(
        &self,
        ty: Type,
    ) -> Option<Vec<(Type, bool)>> {
        if ty.is_any() {
            return Some(vec![(ty, false)]);
        }
        match self.node(ty).as_deref()? {
            Node::Tuple(items) => {
                let mut targets = Vec::new();
                for &item in items {
                    targets.extend(self.runtime_type_ranges(item)?);
                }
                Some(targets)
            }
            Node::Container(Container::TypeObject, items) => {
                let target = items[0];
                let precise = target == Type::None
                    || self
                        .class_id(target)
                        .is_some_and(|id| self.class(id).final_class);
                Some(vec![(target, !precise)])
            }
            Node::Class(id)
            | Node::AppliedClass {
                id, value: true, ..
            } => {
                let target = self.builtin_scalar(ty).unwrap_or_else(|| {
                    self.applied_class(
                        *id,
                        vec![Type::AnySpecial; self.class(*id).generic.len()],
                        false,
                    )
                });
                Some(vec![(target, false)])
            }
            Node::Callable(signature) if signature.constructor => signature
                .returns
                .map(|ty| vec![(self.runtime_erased_type(ty), false)]),
            Node::Variable(variable) => self.runtime_type_ranges(variable.bound),
            Node::Union(items) => {
                let mut targets = Vec::new();
                for &item in items {
                    targets.extend(self.runtime_type_ranges(item)?);
                }
                for (_, upper) in &mut targets {
                    *upper = true;
                }
                Some(targets)
            }
            _ => None,
        }
    }

    fn runtime_erased_type(&self, ty: Type) -> Type {
        match self.node(ty).as_deref() {
            Some(Node::Container(kind, items)) => {
                self.intern(Node::Container(*kind, vec![Type::AnySpecial; items.len()]))
            }
            Some(Node::AppliedClass { id, arguments, .. }) => {
                self.applied_class(*id, vec![Type::AnySpecial; arguments.len()], false)
            }
            _ => ty,
        }
    }

    pub(in crate::infrastructure::mypy) fn runtime_instance_narrow(
        &self,
        input: Type,
        targets: &[(Type, bool)],
        positive: bool,
        module: &str,
    ) -> Type {
        if input == Type::Never {
            return input;
        }
        if self
            .node(input)
            .is_some_and(|node| matches!(node.as_ref(), Node::Variable(_)))
        {
            return input;
        }
        if let Some(Node::Union(items)) = self.node(input).as_deref() {
            return self.union(
                items
                    .iter()
                    .map(|&item| self.runtime_instance_narrow(item, targets, positive, module))
                    .collect(),
            );
        }
        if positive {
            let mut result = self.union(
                targets
                    .iter()
                    .map(|&(target, _)| self.runtime_instance_item(input, target, true, module))
                    .collect(),
            );
            if (input == Type::Bool
                || self
                    .node(input)
                    .is_some_and(|node| matches!(node.as_ref(), Node::Literal(Literal::Bool(_)))))
                && targets.iter().any(|&(target, _)| target == Type::Bool)
            {
                result = Type::Bool;
            }
            result
        } else {
            targets
                .iter()
                .filter(|(_, upper)| !upper)
                .fold(input, |input, &(target, _)| {
                    self.runtime_instance_item(input, target, false, module)
                })
        }
    }

    fn runtime_instance_item(
        &self,
        input: Type,
        target: Type,
        positive: bool,
        module: &str,
    ) -> Type {
        if target.is_any() {
            return if positive { target } else { input };
        }
        if matches!(input, Type::Any | Type::AnySpecial | Type::Object) {
            return if positive { target } else { input };
        }
        if let Some(Node::Literal(value) | Node::LiteralValue(value)) = self.node(input).as_deref()
        {
            let narrowed = self.runtime_instance_item(value.base(), target, positive, module);
            return if narrowed == Type::Never || matches!(value, Literal::Bool(_)) {
                narrowed
            } else {
                input
            };
        }
        let erased_builtin_container = match self.node(target).as_deref() {
            Some(Node::Container(kind, arguments))
                if matches!(
                    kind,
                    Container::List | Container::Dict | Container::Set | Container::FrozenSet
                ) && arguments.len() == if *kind == Container::Dict { 2 } else { 1 }
                    && arguments.iter().all(|&argument| argument.is_any()) =>
            {
                Some(*kind)
            }
            Some(Node::AppliedClass {
                id,
                arguments,
                value: false,
            }) if arguments.len() == self.class(*id).generic.len()
                && arguments.iter().all(|&argument| argument.is_any()) =>
            {
                match self.class(*id).namespace.as_str() {
                    "builtins.list" => Some(Container::List),
                    "builtins.dict" => Some(Container::Dict),
                    "builtins.set" => Some(Container::Set),
                    "builtins.frozenset" => Some(Container::FrozenSet),
                    _ => None,
                }
            }
            Some(Node::Instance(id)) if self.class(*id).generic.is_empty() => {
                match self.class(*id).namespace.as_str() {
                    "builtins.list" => Some(Container::List),
                    "builtins.dict" => Some(Container::Dict),
                    "builtins.set" => Some(Container::Set),
                    "builtins.frozenset" => Some(Container::FrozenSet),
                    _ => None,
                }
            }
            _ => None,
        };
        let same_erased_builtin_container = erased_builtin_container.is_some_and(|kind| {
            self.node(input).is_some_and(|node| {
                matches!(node.as_ref(), Node::Container(input_kind, _) if *input_kind == kind)
            })
        });
        let covered = input == target
            || same_erased_builtin_container
            || target == Type::Object
            || (input == Type::Bool && target == Type::Int)
            || (self.node(input).is_some()
                && self.node(target).is_some()
                && self
                    .class_id(input)
                    .zip(self.class_id(target))
                    .is_some_and(|(input, target)| self.nominal_accepts(target, input)))
            || self
                .class_id(target)
                .is_some_and(|id| self.class(id).protocol && self.accepts(target, input));
        if !positive {
            return if covered { Type::Never } else { input };
        }
        if covered {
            return input;
        }
        if input == Type::Int && target == Type::Bool {
            return target;
        }
        if self.accepts(input, target) {
            return target;
        }
        if self.accepts(target, input) {
            return input;
        }
        self.nominal_intersection(input, target, module)
            .unwrap_or(Type::Never)
    }

    pub(in crate::infrastructure::mypy) fn runtime_class_narrow(
        &self,
        input: Type,
        targets: &[(Type, bool)],
        positive: bool,
        module: &str,
    ) -> Type {
        if let Some(Node::Union(items)) = self.node(input).as_deref() {
            return self.union(
                items
                    .iter()
                    .map(|&item| self.runtime_class_narrow(item, targets, positive, module))
                    .collect(),
            );
        }
        let input_node = self.node(input);
        let Some(Node::Container(Container::TypeObject, items)) = input_node.as_deref() else {
            return input;
        };
        let narrowed = self.runtime_instance_narrow(items[0], targets, positive, module);
        if narrowed == Type::Never {
            narrowed
        } else {
            self.intern(Node::Container(Container::TypeObject, vec![narrowed]))
        }
    }
}
