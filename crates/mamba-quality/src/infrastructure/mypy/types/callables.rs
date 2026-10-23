//! Callable annotations and function values share interned signatures.
use super::super::signatures::{Kind, Parameter, Signature};
use super::*;
use mamba_language::application::quality_syntax::PythonTarget;
use std::sync::Arc;

impl Context {
    pub(super) fn bare_callable(&self, _variables: &VariableMap) -> Type {
        self.callable_type(Arc::new(Signature {
            name: None,
            parameters: [Kind::VariadicPositional, Kind::VariadicKeyword]
                .into_iter()
                .map(|kind| Parameter {
                    name: String::new(),
                    ty: Some(Type::Any),
                    kind,
                    required: false,
                })
                .collect::<Vec<_>>()
                .into(),
            returns: Some(Type::Any),
            narrowing: None,
            return_explicit: true,
            implicit: false,
            plain_source_implicit_name: false,
            ignored: false,
            variables: Variables::new(),
            generic: Arc::from([]),
            owner: None,
            constructor: false,
            ellipsis: true,
            overloads: Arc::from([]),
            bound_receiver: None,
            definition_module: None,
            native_symbol: None,
        }))
    }

    pub(in crate::infrastructure::mypy) fn callable_type(&self, signature: Arc<Signature>) -> Type {
        self.intern(Node::Callable(signature))
    }

    pub(in crate::infrastructure::mypy) fn callable_signature(
        &self,
        ty: Type,
    ) -> Option<Arc<Signature>> {
        self.callable_signature_for(ty, PythonTarget::Py312)
    }

    pub(in crate::infrastructure::mypy) fn callable_signature_for(
        &self,
        ty: Type,
        target: PythonTarget,
    ) -> Option<Arc<Signature>> {
        if let Some(node) = self.node(ty) {
            if let Node::Callable(signature) = node.as_ref() {
                return Some(signature.clone());
            }
        }
        self.builtin_alias_constructor(ty, target)
            .or_else(|| self.class_constructor_for(ty, target))
    }

    pub(in crate::infrastructure::mypy) fn union_callable_signature_for(
        &self,
        ty: Type,
        target: PythonTarget,
    ) -> Option<Arc<Signature>> {
        if self.is_class_value(ty) {
            if let Some(signature) = self
                .builtin_alias_constructor(ty, target)
                .or_else(|| self.builtin_value_constructor(ty, target))
            {
                return Some(signature);
            }
            let id = self.class_id(ty)?;
            if self.constructor_member(ty).is_none()
                && !self.class(id).protocol
                && self.abstract_members(id).is_empty()
            {
                // A placeholder ID alone is not evidence for a default constructor.
                return None;
            }
        }
        self.callable_signature_for(ty, target)
    }

    pub(in crate::infrastructure::mypy) fn non_callable_rule_for(
        &self,
        ty: Type,
        target: PythonTarget,
    ) -> Option<&'static str> {
        if ty == Type::None {
            return Some("misc");
        }
        let scalar = match self.node(ty).as_deref() {
            Some(Node::Literal(value) | Node::LiteralValue(value)) => value.base(),
            _ => ty,
        };
        self.missing_builtin_attribute(scalar, "__call__", target)
            .map(|_| "operator")
    }

    pub(super) fn callable_annotation(
        &self,
        arguments: &[&Expr],
        variables: &VariableMap,
    ) -> Option<Type> {
        if let Some(ty) = self.returned_callable_annotation(arguments, variables) {
            return Some(ty);
        }
        let [parameters, returns] = arguments else {
            return None;
        };
        let narrowing = self.predicate_annotation(returns, variables);
        let returns = self.annotation_with_variables(returns, variables)?;
        let (parameters, ellipsis) = self.callable_parameters(parameters, variables)?;
        Some(self.callable_type(Arc::new(Signature {
            name: None,
            parameters: parameters.into(),
            returns: Some(returns),
            narrowing,
            return_explicit: true,
            implicit: false,
            plain_source_implicit_name: false,
            ignored: false,
            // The resolved types retain their enclosing binders. An annotation
            // has no function body that needs the lexical annotation scope.
            variables: Variables::new(),
            // The fallback retains the old enclosing-binder policy. A proved
            // returned ordinary Callable was handled above.
            generic: Arc::from([]),
            owner: None,
            constructor: false,
            ellipsis,
            overloads: Arc::from([]),
            bound_receiver: None,
            definition_module: None,
            native_symbol: None,
        })))
    }

    pub(in crate::infrastructure::mypy) fn assignment_callable_names(
        &self,
        actual: Type,
        expected: Type,
    ) -> Option<(String, String)> {
        let actual_node = self.node(actual)?;
        let expected_node = self.node(expected)?;
        let (Node::Callable(actual_signature), Node::Callable(expected_signature)) =
            (actual_node.as_ref(), expected_node.as_ref())
        else {
            return None;
        };
        // Keep alias-origin assignment spelling and non-callable display policies.
        if self.alias_application(actual).is_some() || self.alias_application(expected).is_some() {
            return None;
        }
        let ordinary = |signature: &Signature| {
            !signature.ellipsis
                && signature.generic.is_empty()
                && signature.overloads.is_empty()
                && signature.narrowing.is_none()
                && !signature.constructor
                && !signature.ignored
                && signature.owner.is_none()
                && signature.bound_receiver.is_none()
                && signature.returns.is_some()
                && signature
                    .name
                    .as_deref()
                    .is_some_and(|name| !name.is_empty() && !name.chars().any(char::is_whitespace))
                && signature.parameters.iter().all(|parameter| {
                    parameter.kind == Kind::PositionalOrKeyword
                        && !parameter.name.is_empty()
                        && parameter.ty.is_some()
                })
        };
        if !ordinary(actual_signature)
            || !ordinary(expected_signature)
            || expected_signature.parameters.is_empty()
            || actual_signature.parameters.len() < expected_signature.parameters.len()
        {
            return None;
        }
        // The native comparison below pairs parameters by position. Preserve the
        // original name-first rule when names point to another position. Reject
        // duplicate names because their correspondence is not unambiguous.
        let names_are_unique = |signature: &Signature| {
            signature
                .parameters
                .iter()
                .enumerate()
                .all(|(index, parameter)| {
                    signature.parameters[index + 1..]
                        .iter()
                        .all(|other| parameter.name != other.name)
                })
        };
        if !names_are_unique(actual_signature)
            || !names_are_unique(expected_signature)
            || expected_signature
                .parameters
                .iter()
                .enumerate()
                .any(|(expected_index, expected)| {
                    actual_signature
                        .parameters
                        .iter()
                        .enumerate()
                        .any(|(actual_index, actual)| {
                            actual_index != expected_index && actual.name == expected.name
                        })
                })
        {
            return None;
        }
        // Equal payload IDs take accepts' equality path without instantiation,
        // alias expansion, or a new type graph. This is a display-only subset.
        let identical = |left: Option<Type>, right: Option<Type>| {
            left == right && left.is_some_and(|ty| !self.is_alias_instance(ty))
        };
        if !identical(expected_signature.returns, actual_signature.returns)
            || !expected_signature
                .parameters
                .iter()
                .zip(actual_signature.parameters.iter())
                .all(|(expected, actual)| identical(expected.ty, actual.ty))
            || !self.accepts_callable(expected_signature, actual_signature, true)
        {
            return None;
        }
        Some((
            self.pretty_callable(actual_signature),
            self.pretty_callable(expected_signature),
        ))
    }

    pub(super) fn callable_name(&self, signature: &Signature) -> String {
        if !signature.overloads.is_empty() {
            return "overloaded function".into();
        }
        if !signature.ellipsis
            && signature.parameters.iter().any(|parameter| {
                !matches!(
                    parameter.kind,
                    Kind::PositionalOnly | Kind::PositionalOrKeyword
                )
            })
        {
            return self.pretty_callable(signature);
        }
        let parameters = if signature.ellipsis {
            "...".into()
        } else {
            format!(
                "[{}]",
                signature
                    .parameters
                    .iter()
                    .map(|parameter| self.name(parameter.ty.unwrap_or(Type::Any)))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        format!(
            "Callable[{parameters}, {}]",
            signature.narrowing.map_or_else(
                || self.name(signature.returns.unwrap_or(Type::Any)),
                |(kind, ty)| format!("{}[{}]", kind.name(), self.name(ty)),
            )
        )
    }

    pub(crate) fn pretty_callable(&self, signature: &Signature) -> String {
        let mut arguments = Vec::new();
        if let Some(receiver) = &signature.bound_receiver {
            arguments.push(receiver.clone());
        }
        let mut star = false;
        for (index, parameter) in signature.parameters.iter().enumerate() {
            if parameter.kind == Kind::KeywordOnly && !star {
                arguments.push("*".into());
                star = true;
            }
            let prefix = match parameter.kind {
                Kind::VariadicPositional => {
                    star = true;
                    "*"
                }
                Kind::VariadicKeyword => "**",
                _ => "",
            };
            let ty = self.name(parameter.ty.unwrap_or(Type::Any));
            let mut argument =
                if parameter.name.is_empty() || parameter.kind == Kind::PositionalOnly {
                    format!("{prefix}{ty}")
                } else {
                    format!("{prefix}{}: {ty}", parameter.name)
                };
            if !parameter.required
                && matches!(
                    parameter.kind,
                    Kind::PositionalOnly | Kind::PositionalOrKeyword | Kind::KeywordOnly
                )
            {
                argument.push_str(" = ...");
            }
            arguments.push(argument);
            if parameter.kind == Kind::PositionalOnly
                && signature
                    .parameters
                    .get(index + 1)
                    .is_none_or(|parameter| parameter.kind != Kind::PositionalOnly)
            {
                arguments.push("/".into());
            }
        }
        let generic = if signature.generic.is_empty() {
            String::new()
        } else {
            format!(
                "[{}] ",
                signature
                    .generic
                    .iter()
                    .map(|&ty| self.callable_parameter_name(ty))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        format!(
            "def {generic}{}({}) -> {}",
            signature.name.as_deref().unwrap_or(""),
            arguments.join(", "),
            self.name(signature.returns.unwrap_or(Type::Any))
        )
    }

    fn callable_parameter_name(&self, ty: Type) -> String {
        if let Some(Node::Variable(variable)) = self.node(ty).as_deref() {
            if !variable.parameter_spec && !variable.type_var_tuple {
                if variable.bound != Type::Object {
                    return format!("{}: {}", variable.name, self.name(variable.bound));
                }
                if !variable.values.is_empty() {
                    return format!(
                        "{}: ({})",
                        variable.name,
                        variable
                            .values
                            .iter()
                            .map(|&ty| self.name(ty))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
            }
        }
        self.name(ty)
    }

    pub(in crate::infrastructure::mypy) fn substitute_callable(
        &self,
        signature: &Signature,
        solutions: &BTreeMap<Type, Type>,
    ) -> Signature {
        let (parameters, ellipsis) = self.substitute_parameters(signature, solutions);
        let mut generic = signature
            .generic
            .iter()
            .copied()
            .filter(|ty| !solutions.contains_key(ty))
            .collect::<Vec<_>>();
        if signature.generic.is_empty()
            && self
                .signature_parameter_spec(signature)
                .is_some_and(|(_, variable)| {
                    solutions
                        .get(&variable)
                        .and_then(|&ty| self.node(ty))
                        .is_some_and(|node| matches!(node.as_ref(), Node::Parameters(_, _)))
                })
        {
            let mut free = Vec::new();
            for parameter in &parameters {
                if let Some(ty) = parameter.ty {
                    self.collect_variables(ty, &mut free);
                }
            }
            if let Some(ty) = signature.returns {
                self.collect_variables(self.substitute(ty, solutions), &mut free);
            }
            generic.extend(
                free.into_iter()
                    .filter(|variable| solutions.get(variable) == Some(variable)),
            );
        }
        Signature {
            parameters: parameters.into(),
            ellipsis,
            returns: signature.returns.map(|ty| self.substitute(ty, solutions)),
            narrowing: signature
                .narrowing
                .map(|(kind, ty)| (kind, self.substitute(ty, solutions))),
            generic: generic.into(),
            overloads: signature
                .overloads
                .iter()
                .map(|signature| Arc::new(self.substitute_callable(signature, solutions)))
                .collect::<Vec<_>>()
                .into(),
            ..signature.clone()
        }
    }
}

#[cfg(test)]
mod tests;
