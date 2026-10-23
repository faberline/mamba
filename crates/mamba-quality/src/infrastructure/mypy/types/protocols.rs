//! Protocol compatibility compares declared members independently from nominal bases.
use super::*;

#[derive(Default)]
struct Comparison {
    members: usize,
    missing: Vec<String>,
    conflicts: Vec<String>,
    flags: Vec<String>,
}

impl Comparison {
    fn compatible(&self) -> bool {
        self.missing.is_empty() && self.conflicts.is_empty() && self.flags.is_empty()
    }
}

impl Context {
    fn protocol_self_method(
        &self,
        expected: Type,
        actual: Type,
        name: &str,
    ) -> Option<super::method_bindings::BoundMethod> {
        let owner = self.class_member_owner(expected, name)?;
        let class = self.class(owner);
        let method = class.methods.get(name)?;
        let receiver = method.signature.parameters.first()?.ty?;
        if method.kind == super::super::classes::MethodKind::Instance
            && method.signature.generic.contains(&receiver)
        {
            // Keep owner arguments on expected, but infer method self from actual.
            // Binding expected first would consume the receiver variable too soon.
            return self.bind_class_method_at(
                expected,
                actual,
                owner,
                name,
                mamba_language::application::quality_syntax::PythonTarget::Py312,
            );
        }
        self.class_method(expected, name)
            .map(super::method_bindings::BoundMethod::plain)
    }
    pub(in crate::infrastructure::mypy) fn protocol_parameter_variance(
        &self,
        id: u32,
        index: usize,
    ) -> ParameterVariance {
        let dummy = self.declare_class("<mamba-variance>", "<dummy>", 0);
        let mut lower = vec![self.class(dummy).instance; self.class(id).generic.len()];
        let mut upper = lower.clone();
        lower[index] = Type::Never;
        upper[index] = Type::Object;
        let lower = self.applied_class(id, lower, false);
        let upper = self.applied_class(id, upper, false);
        if self.protocol_accepts(upper, lower) {
            ParameterVariance::Covariant
        } else if self.protocol_accepts(lower, upper) {
            ParameterVariance::Contravariant
        } else {
            ParameterVariance::Invariant
        }
    }

    fn protocol_callable_display(
        &self,
        signature: &super::super::signatures::Signature,
        class_value: bool,
    ) -> String {
        if !signature.overloads.is_empty() {
            return signature
                .overloads
                .iter()
                .map(|variant| {
                    format!(
                        "@overload\n{}",
                        self.protocol_callable_display(variant, class_value)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
        }
        let mut signature = signature.clone();
        if class_value {
            signature.bound_receiver = None;
        }
        self.pretty_callable(&signature)
    }

    fn protocol_callable_conflict(
        &self,
        parent: &super::super::signatures::Signature,
        child: &super::super::signatures::Signature,
        class_value: bool,
    ) -> String {
        let display = |signature| {
            self.protocol_callable_display(signature, class_value)
                .replace('\n', "\n        ")
        };
        format!(
            "    Expected:\n        {}\n    Got:\n        {}",
            display(parent),
            display(child)
        )
    }

    pub(in crate::infrastructure::mypy) fn protocol_constraint_members(
        &self,
        expected: Type,
        actual: Type,
    ) -> Vec<(Type, Type, bool)> {
        let Some(id) = self.class_id(expected) else {
            return Vec::new();
        };
        let mut pairs = Vec::new();
        for name in self.protocol_members(id) {
            if let Some(parent) = self.protocol_self_method(expected, actual, &name) {
                // Rejected receivers supply no inference constraints. Acceptance
                // records their error as a conflict in protocol_comparison.
                if parent.error.is_some() {
                    continue;
                }
                let parent = parent.signature;
                let child = if name == "__call__" {
                    self.callable_signature(actual)
                } else {
                    self.class_method(actual, &name)
                };
                if let Some(child) = child {
                    pairs.push((self.callable_type(parent), self.callable_type(child), false));
                }
            } else if let (Some(parent), Some(child)) = (
                self.class_field(expected, &name),
                self.class_field(actual, &name),
            ) {
                pairs.push((
                    parent.ty.unwrap_or(Type::Any),
                    child.ty.unwrap_or(Type::Any),
                    !parent.readonly,
                ));
            }
        }
        pairs
    }

    fn protocol_comparison(&self, expected: Type, actual: Type) -> Comparison {
        let id = self
            .class_id(expected)
            .expect("Protocol type has a class declaration");
        let class = self.class(id);
        let members = self.protocol_members(id);
        let mut comparison = Comparison {
            members: members.len(),
            ..Comparison::default()
        };
        let class_value = self.is_class_value(actual);
        for name in members {
            let is_static = |kind| {
                matches!(
                    kind,
                    Some(
                        super::super::classes::MethodKind::Static
                            | super::super::classes::MethodKind::Class
                    )
                )
            };
            if is_static(self.class_method_kind(expected, &name))
                && !is_static(self.class_method_kind(actual, &name))
            {
                comparison.flags.push(format!(
                    "Protocol member {}.{name} expected class or static method",
                    class.name
                ));
            }
            if let Some(parent) = self.protocol_self_method(expected, actual, &name) {
                if let Some(error) = parent.error {
                    comparison.conflicts.push(error);
                    continue;
                }
                let parent = parent.signature;
                let child = if name == "__call__" {
                    self.callable_signature(actual)
                } else {
                    self.class_method(actual, &name)
                };
                if let Some(child) = child {
                    if !self.accepts_callable(&parent, &child, name != "__call__") {
                        comparison.conflicts.push(self.protocol_callable_conflict(
                            &parent,
                            &child,
                            class_value,
                        ));
                    }
                } else if let Some(field) = self.class_field(actual, &name) {
                    if field.ty.is_some_and(|ty| ty.is_any()) {
                        continue;
                    } else if let Some(child) = field.ty.and_then(|ty| self.callable_signature(ty))
                    {
                        if !self.accepts_callable(&parent, &child, name != "__call__") {
                            comparison.conflicts.push(self.protocol_callable_conflict(
                                &parent,
                                &child,
                                class_value,
                            ));
                        }
                    } else {
                        comparison.conflicts.push(format!(
                            "    {name}: expected \"{}\", got \"{}\"",
                            self.callable_name(&parent),
                            self.name(field.ty.unwrap_or(Type::Any))
                        ));
                    }
                } else {
                    comparison.missing.push(name);
                }
            } else if let Some(parent) = self.class_field(expected, &name) {
                let child = if self.typed_dict(actual).is_some() {
                    None
                } else {
                    self.class_field(actual, &name)
                };
                if let Some(child) = child {
                    let parent_ty = parent.ty.unwrap_or(Type::Any);
                    let child_ty = child.ty.unwrap_or(Type::Any);
                    if !self.accepts(parent_ty, child_ty)
                        || (!parent.readonly && !self.accepts(child_ty, parent_ty))
                    {
                        if let (Some(parent), Some(child)) = (
                            self.callable_signature(parent_ty),
                            self.callable_signature(child_ty),
                        ) {
                            comparison.conflicts.push(self.protocol_callable_conflict(
                                &parent,
                                &child,
                                class_value,
                            ));
                        } else {
                            comparison.conflicts.push(format!(
                                "    {name}: expected \"{}\", got \"{}\"",
                                self.name(parent_ty),
                                self.name(child_ty)
                            ));
                        }
                    }
                    if !parent.readonly && child.readonly {
                        comparison.flags.push(format!("Protocol member {}.{name} expected settable variable, got read-only attribute", class.name));
                    }
                    if class_value {
                        if !child.class_var {
                            comparison.flags.push(format!("Only class variables allowed for class object access on protocols, {name} is an instance variable of \"{}\"", self.class(self.class_id(actual).unwrap()).name));
                        }
                        if parent.class_var {
                            comparison.flags.push(format!("ClassVar protocol member {}.{name} can never be matched by a class object", class.name));
                        }
                    } else if parent.class_var != child.class_var {
                        comparison.flags.push(format!(
                            "Protocol member {}.{name} expected {} variable, got {} variable",
                            class.name,
                            if parent.class_var {
                                "class"
                            } else {
                                "instance"
                            },
                            if child.class_var { "class" } else { "instance" }
                        ));
                    }
                } else if let Some(child) = self.class_method(actual, &name) {
                    let parent_ty = parent.ty.unwrap_or(Type::Any);
                    if let Some(parent) = self.callable_signature(parent_ty) {
                        comparison.conflicts.push(self.protocol_callable_conflict(
                            &parent,
                            &child,
                            class_value,
                        ));
                    } else {
                        comparison.conflicts.push(format!(
                            "    {name}: expected \"{}\", got \"{}\"",
                            self.name(parent_ty),
                            self.callable_name(&child)
                        ));
                    }
                    if !parent.readonly {
                        comparison.flags.push(format!("Protocol member {}.{name} expected settable variable, got read-only attribute", class.name));
                    }
                } else {
                    comparison.missing.push(name);
                }
            }
        }
        comparison
    }

    pub(in crate::infrastructure::mypy) fn protocol_accepts(
        &self,
        expected: Type,
        actual: Type,
    ) -> bool {
        let pair = (expected, actual);
        if !self.classes.borrow_mut().protocol_checks.insert(pair) {
            return true;
        }
        let compatible = self.protocol_comparison(expected, actual).compatible();
        self.classes.borrow_mut().protocol_checks.remove(&pair);
        compatible
    }

    pub(in crate::infrastructure::mypy) fn protocol_hint(
        &self,
        expected: Type,
        actual: Type,
    ) -> Option<String> {
        if let Some(Node::Union(items)) = self.node(expected).as_deref() {
            return items
                .iter()
                .find_map(|&item| self.protocol_hint(item, actual));
        }
        let id = self.class_id(expected)?;
        let class = self.class(id);
        if !class.protocol {
            return None;
        }
        if self.is_class_value(actual) && self.builtin_scalar(actual).is_some() {
            return None;
        }
        if self.collection_has_nominal_base(actual, id) {
            return None;
        }
        if self.class_arguments(expected).contains(&Type::Never) {
            return None;
        }
        let pair = (expected, actual);
        if !self.classes.borrow_mut().protocol_checks.insert(pair) {
            return None;
        }
        let mut comparison = self.protocol_comparison(expected, actual);
        self.classes.borrow_mut().protocol_checks.remove(&pair);
        if !class.generic.is_empty()
            && self
                .class_id(actual)
                .is_some_and(|id| !self.class(id).generic.is_empty())
            && self.protocol_accepts(
                self.applied_class(id, vec![Type::Any; class.generic.len()], false),
                actual,
            )
        {
            comparison.conflicts.clear();
        }
        if self.callable_signature(actual).is_some() && self.class_id(actual).is_none() {
            let signature = self.class_method(expected, "__call__")?;
            return Some(format!(
                "\"{}.__call__\" has type \"{}\"",
                self.name(expected),
                self.pretty_callable(&signature)
            ));
        }
        let mut hints = Vec::new();
        let actual_name = self.class_id(actual).map_or_else(
            || self.collection_conflict_name(actual),
            |id| {
                if self.is_class_value(actual) {
                    self.class(id).name.clone()
                } else {
                    self.name(actual)
                }
            },
        );
        if !comparison.missing.is_empty() && comparison.missing.len() < comparison.members {
            let missing_name = self.class_id(actual).map_or_else(
                || match self.node(actual).as_deref() {
                    Some(Node::Container(kind, _)) => kind.name().into(),
                    Some(Node::Tuple(_) | Node::VariadicTuple(_)) => "tuple".into(),
                    _ => actual_name.clone(),
                },
                |id| self.class(id).name.clone(),
            );
            hints.push(format!(
                "\"{}\" is missing following \"{}\" protocol member{}:\n    {}",
                missing_name,
                class.name,
                if comparison.missing.len() == 1 {
                    ""
                } else {
                    "s"
                },
                comparison.missing.join(", ")
            ));
        }
        if !comparison.conflicts.is_empty() {
            hints.push(format!(
                "Following member(s) of \"{}\" have conflicts:\n{}",
                actual_name,
                comparison.conflicts.join("\n")
            ));
        }
        hints.extend(comparison.flags);
        (!hints.is_empty()).then(|| hints.join("\n"))
    }
}
