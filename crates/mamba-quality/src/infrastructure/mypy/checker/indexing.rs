//! Index calls retain their source span and slice-specific diagnostic kind.
use super::*;
use quality_syntax::python_ast::{Expr, ExprSubscript, Number};

impl Checker<'_> {
    pub(super) fn check_nominal_subscript(&mut self, subscript: &ExprSubscript) {
        let Some(receiver) = self
            .scopes
            .subscript_receiver(subscript, &self.environment())
        else {
            return;
        };
        if subscript.ctx.is_load() {
            if let (Some(types::Node::Tuple(items)), Expr::NumberLiteral(number)) = (
                self.context.node(receiver).as_deref(),
                subscript.slice.as_ref(),
            ) {
                if self.context.tuple_pack_position(items).is_none() {
                    if let Number::Int(value) = &number.value {
                        if let Some(index) = value.as_i64().filter(|index| *index >= 0) {
                            let out_of_range =
                                usize::try_from(index).map_or(true, |index| index >= items.len());
                            if out_of_range {
                                self.emit("misc", "Tuple index out of range".into(), subscript);
                                return;
                            }
                        }
                    }
                }
            }
            if receiver == types::Type::Str {
                self.check_builtin_str_index(subscript, receiver);
                return;
            }
            let unindexable = match self.context.node(receiver).as_deref() {
                Some(types::Node::Union(items)) => {
                    items.iter().any(|&item| self.unindexable_union_item(item))
                }
                _ => self.unindexable_standalone_receiver(receiver),
            };
            if unindexable {
                self.emit(
                    "index",
                    format!(
                        "Value of type \"{}\" is not indexable",
                        self.context.name(receiver)
                    ),
                    subscript,
                );
            }
        }
        let Some(call) = self
            .scopes
            .nominal_subscript_call_with_receiver(subscript, receiver)
        else {
            return;
        };
        let Some(signature) = self
            .scopes
            .callable_expression_in(&call.func, &self.environment())
        else {
            return;
        };
        if subscript.slice.is_slice_expr() {
            if let Some(actual) = self.infer(&subscript.slice) {
                let candidates: Vec<_> = if signature.overloads.is_empty() {
                    vec![&*signature]
                } else {
                    signature.overloads.iter().map(AsRef::as_ref).collect()
                };
                let expected = candidates
                    .iter()
                    .filter_map(|variant| variant.parameters.first()?.ty)
                    .find(|&expected| {
                        self.context.class_id(expected).is_some()
                            && self.context.class_id(expected) == self.context.class_id(actual)
                    });
                if let Some(expected) = expected {
                    if !self.context.accepts(expected, actual) {
                        let receiver = self.infer(&subscript.value).unwrap_or(types::Type::Any);
                        self.emit(
                            "index",
                            format!(
                                "Invalid index type \"{}\" for \"{}\"; expected type \"{}\"",
                                self.context.name(actual),
                                self.context.name(receiver),
                                self.context.name(expected)
                            ),
                            subscript.slice.as_ref(),
                        );
                    }
                    return;
                }
            }
        }
        if signature.overloads.is_empty() {
            self.bind_call(&call, &signature);
        } else {
            self.check_overload_call(&call, &signature);
        }
    }

    fn check_builtin_str_index(&mut self, subscript: &ExprSubscript, receiver: types::Type) {
        let Some(actual) = self
            .infer(&subscript.slice)
            .filter(|&actual| !actual.is_any())
        else {
            return;
        };
        if self
            .context
            .runtime_builtin_class_instance("str", self.python_target)
            .is_none()
        {
            return;
        }
        let Some(signature) =
            self.context
                .class_method_for(receiver, "__getitem__", self.python_target)
        else {
            return;
        };
        if !signature.overloads.is_empty() {
            return;
        }
        let [parameter] = signature.parameters.as_ref() else {
            return;
        };
        let Some(expected) = parameter.ty else {
            return;
        };
        if !self.context.accepts(expected, actual) {
            self.emit(
                "index",
                format!(
                    "Invalid index type \"{}\" for \"{}\"; expected type \"{}\"",
                    self.context.name(actual),
                    self.context.name(receiver),
                    self.context.name(expected)
                ),
                subscript.slice.as_ref(),
            );
        }
    }

    fn unindexable_standalone_receiver(&self, ty: types::Type) -> bool {
        if ty == types::Type::None {
            return true;
        }
        // Do not turn an unknown, alias, variable, or compact container into a
        // missing-member proof. Class objects retain their existing route too.
        if ty != types::Type::Object
            && !matches!(
                self.context.node(ty).as_deref(),
                Some(types::Node::Instance(_) | types::Node::AppliedClass { value: false, .. })
            )
        {
            return false;
        }
        let Some(id) = self
            .context
            .class_id(ty)
            .or_else(|| (ty == types::Type::Object).then(|| self.context.builtin_class(ty)))
        else {
            return false;
        };
        let mut complete = BTreeSet::new();
        {
            let registry = self.context.classes.borrow();
            let builtin_members_complete = registry.builtin_value_libraries[match self.python_target
            {
                quality_syntax::PythonTarget::Py312 => 0,
                quality_syntax::PythonTarget::Py313 => 1,
                quality_syntax::PythonTarget::Py314 => 2,
            }];
            let mut active = BTreeSet::new();
            let mut pending = vec![(id, false)];
            // A successful root MRO can hide an ancestor's failed MRO. Check
            // the actual base graph before trusting any member lookup.
            while let Some((parent, finish)) = pending.pop() {
                if finish {
                    active.remove(&parent);
                    complete.insert(parent);
                    continue;
                }
                if complete.contains(&parent) {
                    continue;
                }
                if !active.insert(parent) {
                    return false;
                }
                let Some(class) = registry.entries.get(parent as usize) else {
                    return false;
                };
                if registry.populated_class_bases.get(&parent) != Some(&true)
                    || class.plugin
                    // Empty origins exclude every instance_fallback_to_any
                    // witness; malformed origins stay unproved too.
                    || !class.any_base_origins.is_empty()
                    // Scalar bases can precede their full method declarations.
                    || (!builtin_members_complete
                        && class.scalar_bases.iter().any(|&base| base != types::Type::Object))
                {
                    return false;
                }
                pending.push((parent, true));
                pending.extend(class.bases.iter().rev().map(|&base| (base, false)));
            }
        }
        // Do not hold the registry borrow while the existing MRO cache is used.
        if !complete
            .iter()
            .all(|&parent| self.context.class_mro(parent).is_ok())
        {
            return false;
        }
        let Ok(lineage) = self.context.class_mro(id) else {
            return false;
        };
        if lineage.first() != Some(&id)
            || lineage.len() != complete.len()
            || lineage.iter().copied().collect::<BTreeSet<_>>() != complete
        {
            return false;
        }
        self.unindexable_union_item(ty)
    }

    fn unindexable_union_item(&self, ty: types::Type) -> bool {
        if ty == types::Type::None {
            return true;
        }
        if ty.is_any()
            || self.context.is_class_value(ty)
            || self.context.typed_dict(ty).is_some()
            || self
                .scopes
                .class_member_signature(ty, "__getitem__")
                .is_some()
            || self.context.raw_class_field(ty, "__getitem__").is_some()
            || self
                .scopes
                .class_member_signature(ty, "__getattr__")
                .is_some()
            || self.context.raw_class_field(ty, "__getattr__").is_some()
        {
            return false;
        }
        if let Some(node) = self.context.node(ty) {
            if let types::Node::Union(items) = node.as_ref() {
                return items.iter().any(|&item| self.unindexable_union_item(item));
            }
        }
        let Some(id) = self.context.class_id(ty).or_else(|| {
            self.context
                .builtin_scalar(ty)
                .map(|scalar| self.context.builtin_class(scalar))
        }) else {
            return false;
        };
        let Ok(lineage) = self.context.class_mro(id) else {
            return false;
        };
        let registry = self.context.classes.borrow();
        lineage.iter().all(|parent| {
            registry.populated_class_bases.get(parent) == Some(&true)
                && !registry.entries[*parent as usize].plugin
        })
    }
}
