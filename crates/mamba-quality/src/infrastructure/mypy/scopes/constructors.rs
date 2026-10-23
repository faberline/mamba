//! An inferred constructor needs an annotation when a binder has no input bounds.
use super::*;
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, Visitor},
    ExprCall,
};

struct EmptySetComprehensionWrites<'a> {
    constructor: &'a str,
    invalid: bool,
}

impl<'a> Visitor<'a> for EmptySetComprehensionWrites<'_> {
    fn visit_expr(&mut self, expression: &'a Expr) {
        if matches!(expression, Expr::Named(_))
            || matches!(expression, Expr::Name(name)
                if name.ctx.is_store() && name.id.as_str() == self.constructor)
        {
            self.invalid = true;
        } else if !self.invalid {
            walk_expr(self, expression);
        }
    }
}

// The guard must finish before the copied AST is released, including early returns.
pub(in crate::infrastructure::mypy) struct ExpandedCall {
    observations: Option<types::ObservedCallGuard>,
    call: ExprCall,
}

impl std::ops::Deref for ExpandedCall {
    type Target = ExprCall;

    fn deref(&self) -> &Self::Target {
        &self.call
    }
}

impl Drop for ExpandedCall {
    fn drop(&mut self) {
        drop(self.observations.take());
    }
}

impl Scopes {
    pub(in crate::infrastructure::mypy) fn partial_defaultdict_seed(
        &self,
        expression: &Expr,
        environment: &BTreeMap<String, Type>,
    ) -> Option<(
        u32,
        types::Container,
        Type,
        types::constraints::FactoryAmbiguity,
    )> {
        let Expr::Call(call) = expression else {
            return None;
        };
        if !matches!(call.arguments.args.as_ref(), [value] if !value.is_starred_expr())
            || !call.arguments.keywords.is_empty()
        {
            return None;
        }
        let signature = self.callable_expression_in(&call.func, environment)?;
        let resolved = self.resolve_overload(call, &signature, environment, None);
        if !resolved.matched || resolved.union_limit {
            return None;
        }
        let candidate = resolved.candidate?;
        let factory = resolved.factory_poly?;
        let solution = self.call_solution(call, &candidate, environment, None);
        if solution.factory_poly.as_ref() != Some(&factory)
            || !self
                .context
                .factory_ambiguity_matches(&candidate, &solution, &factory)
            || !solution.failed.is_empty()
            || !solution.invalid.is_empty()
        {
            return None;
        }
        let initial = self.context.substitute(candidate.returns?, &solution.types);
        if initial != resolved.returns {
            return None;
        }
        let node = self.context.node(initial)?;
        let types::Node::AppliedClass {
            id,
            arguments,
            value: false,
        } = node.as_ref()
        else {
            return None;
        };
        // This helper verifies the project/target ClassID registration made only
        // for the exact embedded source, not the current import's spelling.
        if self.context.embedded_source_class_name(*id).as_deref()
            != Some("collections.defaultdict")
            || arguments.as_slice() != [Type::Never, factory.ambiguous_value]
        {
            return None;
        }
        let class = self.context.class(*id);
        if class.generic.len() != 2
            || class.plugin
            || !class.any_base_origins.is_empty()
            || class.namespace.split_once(':')?.0 != self.namespace.split_once(':')?.0
            || self.context.module_target(&class.namespace)
                != self.context.module_target(&self.namespace)
        {
            return None;
        }
        let node = self.context.node(factory.ambiguous_value)?;
        let types::Node::Container(
            family @ (types::Container::List | types::Container::Set),
            items,
        ) = node.as_ref()
        else {
            return None;
        };
        if items.as_slice() != [Type::Never] {
            return None;
        }
        Some((*id, *family, initial, factory))
    }

    // Only this empty constructor value supplies an ambiguous bottom. Keep
    // unrelated Never bounds, including the actual dictionary key, unchanged.
    pub(in crate::infrastructure::mypy) fn recover_empty_set_comprehension(
        &self,
        expression: &Expr,
        actual: Option<Type>,
        environment: &BTreeMap<String, Type>,
    ) -> Option<Type> {
        let Expr::DictComp(dictionary) = expression else {
            return None;
        };
        if dictionary.generators.is_empty()
            || dictionary
                .generators
                .iter()
                .any(|generator| generator.is_async)
        {
            return None;
        }
        let Expr::Call(call) = dictionary.value.as_ref() else {
            return None;
        };
        let Expr::Name(factory) = call.func.as_ref() else {
            return None;
        };
        if !call.arguments.args.is_empty() || !call.arguments.keywords.is_empty() {
            return None;
        }
        // This is a read-only AST scan. It performs no inference or Checker
        // visits. No generator target or walrus may replace the outer proof.
        let mut writes = EmptySetComprehensionWrites {
            constructor: factory.id.as_str(),
            invalid: false,
        };
        writes.visit_expr(expression);
        if writes.invalid {
            return None;
        }
        let node = self.context.node(actual?)?;
        let types::Node::Container(types::Container::Dict, arguments) = node.as_ref() else {
            return None;
        };
        let &[key, value] = arguments.as_slice() else {
            return None;
        };
        if matches!(key, Type::Never | Type::Any | Type::AnySpecial | Type::None) {
            return None;
        }
        let value_node = self.context.node(value)?;
        if !matches!(value_node.as_ref(),
            types::Node::Container(types::Container::Set, arguments)
                if arguments.as_slice() == [Type::Never])
            || self.incomplete_collection_constructor(&dictionary.value, Some(value), environment)
                != Some(types::Container::Set)
        {
            return None;
        }
        let recovered = self.context.intern(types::Node::Container(
            types::Container::Set,
            vec![Type::Any],
        ));
        Some(self.context.intern(types::Node::Container(
            types::Container::Dict,
            vec![key, recovered],
        )))
    }

    pub(in crate::infrastructure::mypy) fn incomplete_collection_constructor(
        &self,
        expression: &Expr,
        actual: Option<Type>,
        environment: &BTreeMap<String, Type>,
    ) -> Option<types::Container> {
        let Expr::Call(call) = expression else {
            return None;
        };
        let signature = self.callable_expression_in(&call.func, environment)?;
        if (!signature.constructor && signature.name.as_deref() != Some("sorted"))
            || signature.definition_module.as_deref() != Some("builtins")
        {
            return None;
        }
        let kind = match signature.name.as_deref() {
            Some("list" | "sorted") => types::Container::List,
            Some("dict") => types::Container::Dict,
            Some("set") => types::Container::Set,
            _ => return None,
        };
        matches!(self.context.node(actual?).as_deref(),
            Some(types::Node::Container(actual, arguments))
                if *actual == kind && arguments.contains(&Type::Never))
        .then_some(kind)
    }

    pub(in crate::infrastructure::mypy) fn collection_expanded_call(
        &self,
        call: &ExprCall,
        signature: &Signature,
    ) -> Option<ExpandedCall> {
        if (!signature.constructor
            && !matches!(signature.name.as_deref(), Some("len" | "sorted" | "sum")))
            || signature.definition_module.as_deref() != Some("builtins")
            || !matches!(
                signature.name.as_deref(),
                Some(
                    "list"
                        | "dict"
                        | "set"
                        | "frozenset"
                        | "tuple"
                        | "len"
                        | "reversed"
                        | "enumerate"
                        | "range"
                        | "sorted"
                        | "sum"
                        | "zip"
                )
            )
            || !call.arguments.args.iter().any(Expr::is_starred_expr)
        {
            return None;
        }
        let mut arguments = Vec::new();
        let mut original_arguments = Vec::new();
        for argument in &call.arguments.args {
            if let Expr::Starred(starred) = argument {
                let items = match starred.value.as_ref() {
                    Expr::Tuple(tuple) => &tuple.elts,
                    _ => return None,
                };
                arguments.extend(items.iter().cloned());
                original_arguments.extend(items.iter());
            } else {
                arguments.push(argument.clone());
                original_arguments.push(argument);
            }
        }
        let mut expanded = call.clone();
        expanded.arguments.args = arguments.into();
        let observations =
            self.context
                .observe_call_expansion(call, &original_arguments, &expanded);
        Some(ExpandedCall {
            observations,
            call: expanded,
        })
    }
    pub(in crate::infrastructure::mypy) fn class_alias_reveal(&self, value: &Expr) -> Option<Type> {
        let marker = match value {
            Expr::Name(name) => self.variables().get(name.id.as_str()).copied(),
            Expr::Attribute(_) => self.module_member(value).and_then(|export| export.alias),
            _ => None,
        }?;
        let alias = self.context.alias(self.context.alias_id(marker)?);
        if alias.modern {
            return None;
        }
        if self.context.builtin_fixed_tuple_alias(marker) {
            return Some(
                self.context
                    .intern(types::Node::TypingForm("typing._SpecialForm".into())),
            );
        }
        if let Some(signature) = self
            .context
            .builtin_alias_constructor(marker, self.context.module_target(&self.namespace))
        {
            return Some(self.context.callable_type(signature));
        }
        Some(self.context.class(alias.bare_class?).object)
    }

    pub(in crate::infrastructure::mypy) fn class_member_signature(
        &self,
        ty: Type,
        name: &str,
    ) -> Option<Arc<Signature>> {
        self.context
            .class_method_for(ty, name, self.context.module_target(&self.namespace))
    }

    pub(in crate::infrastructure::mypy) fn callable_value_signature(
        &self,
        ty: Type,
    ) -> Option<Arc<Signature>> {
        self.context
            .callable_signature_for(ty, self.context.module_target(&self.namespace))
    }

    pub(in crate::infrastructure::mypy) fn builtin_value_expression(
        &self,
        expression: &Expr,
    ) -> Option<Type> {
        let Expr::Name(name) = expression else {
            return None;
        };
        if self.has_binding(name.id.as_str())
            || (name.id.as_str().starts_with('_') && !name.id.as_str().starts_with("__"))
        {
            return None;
        }
        if let Some(signature) = self.context.collection_constructor(
            name.id.as_str(),
            self.context.module_target(&self.namespace),
        ) {
            return Some(self.context.callable_type(signature));
        }
        if let Some(signature) = self.context.standard_collection_function(
            name.id.as_str(),
            self.context.module_target(&self.namespace),
        ) {
            return Some(self.context.callable_type(signature));
        }
        if let Some(signature) = self.context.aggregation_function(
            name.id.as_str(),
            self.context.module_target(&self.namespace),
        ) {
            return Some(self.context.callable_type(signature));
        }
        if let Some(signature) = self.context.name_builtin_function(
            name.id.as_str(),
            self.context.module_target(&self.namespace),
        ) {
            return Some(self.context.callable_type(signature));
        }
        if let Some(ty) = types::Context::builtin_name(name.id.as_str()) {
            return Some(self.context.class(self.context.builtin_class(ty)).object);
        }
        self.context
            .runtime_builtin_value(
                name.id.as_str(),
                self.context.module_target(&self.namespace),
            )
            .map(|ty| self.context.native_source_value(ty, &self.namespace))
    }

    pub(in crate::infrastructure::mypy) fn builtin_constructor_expression(
        &self,
        expression: &Expr,
    ) -> Option<Arc<Signature>> {
        if let Some(signature) = self
            .constructor_alias_marker(expression)
            .and_then(|marker| {
                self.context
                    .builtin_alias_constructor(marker, self.context.module_target(&self.namespace))
            })
        {
            return Some(signature);
        }
        if let Expr::Subscript(subscript) = expression {
            if let Some(marker) = self.constructor_alias_marker(&subscript.value) {
                let arguments = match subscript.slice.as_ref() {
                    Expr::Tuple(tuple) => tuple
                        .elts
                        .iter()
                        .map(|item| self.annotation(item))
                        .collect::<Option<Vec<_>>>()?,
                    item => vec![self.annotation(item)?],
                };
                if let Some(signature) = self.context.builtin_alias_constructor_with_arguments(
                    marker,
                    arguments,
                    self.context.module_target(&self.namespace),
                ) {
                    return Some(signature);
                }
            }
            let signature = self.callable_expression_in(&subscript.value, &self.environment())?;
            if signature.definition_module.as_deref() != Some("builtins")
                || !matches!(
                    signature.name.as_deref(),
                    Some(
                        "list"
                            | "dict"
                            | "set"
                            | "frozenset"
                            | "tuple"
                            | "reversed"
                            | "enumerate"
                            | "zip"
                    )
                )
            {
                return None;
            }
            let arguments = match subscript.slice.as_ref() {
                Expr::Tuple(tuple) => tuple
                    .elts
                    .iter()
                    .filter(|item| {
                        signature.name.as_deref() != Some("tuple")
                            || !item.is_ellipsis_literal_expr()
                    })
                    .map(|item| self.annotation(item))
                    .collect::<Option<Vec<_>>>()?,
                item => vec![self.annotation(item)?],
            };
            let solutions = signature.generic.iter().copied().zip(arguments).collect();
            return Some(Arc::new(
                self.context.substitute_callable(&signature, &solutions),
            ));
        }
        let Expr::Name(name) = expression else {
            return None;
        };
        if self.has_binding(name.id.as_str()) {
            return None;
        }
        if let Some(signature) = self.context.collection_constructor(
            name.id.as_str(),
            self.context.module_target(&self.namespace),
        ) {
            return Some(signature);
        }
        if let Some(signature) = self.context.standard_collection_function(
            name.id.as_str(),
            self.context.module_target(&self.namespace),
        ) {
            return Some(signature);
        }
        if let Some(signature) = self.context.collection_function(name.id.as_str()) {
            return Some(signature);
        }
        if let Some(signature) = self.context.aggregation_function(
            name.id.as_str(),
            self.context.module_target(&self.namespace),
        ) {
            return Some(signature);
        }
        let ty = types::Context::builtin_name(name.id.as_str())?;
        self.context
            .builtin_constructor(ty, self.context.module_target(&self.namespace))
    }

    pub(in crate::infrastructure::mypy) fn constructor_alias_marker(
        &self,
        expression: &Expr,
    ) -> Option<Type> {
        self.context
            .annotation_binding(expression, &self.variables())
            .or_else(|| {
                self.module_member(expression)
                    .and_then(|export| export.alias)
            })
            .filter(|&ty| self.context.alias_id(ty).is_some())
    }

    pub(in crate::infrastructure::mypy) fn recover_constructor_assignment(
        &self,
        expression: &Expr,
        environment: &BTreeMap<String, Type>,
    ) -> Option<Type> {
        self.constructor_assignment_recovery(expression, environment)
            .1
    }

    pub(in crate::infrastructure::mypy) fn unsolved_constructor(
        &self,
        expression: &Expr,
        environment: &BTreeMap<String, Type>,
    ) -> bool {
        self.constructor_assignment_recovery(expression, environment)
            .0
    }

    fn constructor_assignment_recovery(
        &self,
        expression: &Expr,
        environment: &BTreeMap<String, Type>,
    ) -> (bool, Option<Type>) {
        let Expr::Call(call) = expression else {
            return (false, None);
        };
        let Some(signature) = self.callable_expression_in(&call.func, environment) else {
            return (false, None);
        };
        if signature.definition_module.as_deref() == Some("builtins")
            && matches!(signature.name.as_deref(), Some("reversed" | "zip"))
        {
            return (false, None);
        }
        if signature.definition_module.as_deref() == Some("builtins")
            && matches!(
                signature.name.as_deref(),
                Some("frozenset" | "tuple" | "enumerate")
            )
        {
            let Some(actual) = self.call_return(call, environment) else {
                return (false, None);
            };
            let incomplete = self.context.immutable_collection_bottom(&signature, actual);
            let recovered = (incomplete && signature.returns.is_some()).then(|| {
                self.context
                    .substitute(actual, &BTreeMap::from([(Type::Never, Type::Any)]))
            });
            return (incomplete, recovered);
        }
        if !signature.constructor
            || (signature.definition_module.as_deref() == Some("builtins")
                && matches!(signature.name.as_deref(), Some("list" | "dict" | "set")))
        {
            return (false, None);
        }
        if signature.returns.is_none() {
            return (
                !self
                    .call_solution(call, &signature, environment, None)
                    .unresolved
                    .is_empty(),
                None,
            );
        }
        let mut factory_poly = None;
        let candidate = if signature.overloads.is_empty() {
            Some(signature.clone())
        } else {
            let resolved = self.resolve_overload(call, &signature, environment, None);
            if resolved.matched && !resolved.union_limit {
                factory_poly = resolved.factory_poly;
            }
            resolved.candidate
        };
        let selected = candidate.as_deref().unwrap_or(signature.as_ref());
        let mut solution = self.call_solution(call, selected, environment, None);
        if let Some(receipt) = factory_poly.as_ref() {
            if solution.factory_poly.as_ref() != Some(receipt)
                || !self
                    .context
                    .factory_ambiguity_matches(selected, &solution, receipt)
            {
                // Conflicting inference receipts cannot justify constructor recovery.
                return (true, None);
            }
        }
        if let Some(returns) = candidate.as_ref().and_then(|candidate| candidate.returns) {
            let mut required = Vec::new();
            self.context.collect_variables(returns, &mut required);
            solution
                .unresolved
                .retain(|variable| required.contains(variable));
        }
        if solution.unresolved.is_empty() {
            return (false, None);
        }
        for &variable in &solution.unresolved {
            solution.types.insert(variable, Type::Any);
        }
        let recovered = selected.returns.map(|returns| {
            self.context
                .substitute_inferred_alias(returns, &solution.types)
        });
        (true, recovered)
    }
}
