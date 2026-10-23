//! Direct empty-list bindings keep their lexical origin until inference completes.
use super::*;
use quality_syntax::python_ast::{ExprCall, ExprName, StmtAugAssign};
use ruff_text_size::TextSize;
use types::{Container, Node, Type};
mod augmented;

#[derive(Clone, Copy)]
enum ListValidity {
    Valid,
    Invalid,
    Unsupported(&'static str),
}

impl ListValidity {
    fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::Invalid, _) | (_, Self::Invalid) => Self::Invalid,
            (Self::Unsupported(reason), _) | (_, Self::Unsupported(reason)) => {
                Self::Unsupported(reason)
            }
            _ => Self::Valid,
        }
    }
}

impl Checker<'_> {
    pub(super) fn partial_list_key(&self, name: &str) -> Option<TextSize> {
        let (owner, definition) = self.scopes.inferred_binding_identity(name)?;
        self.partial_sets
            .get(&definition)
            .filter(|partial| {
                partial.active
                    && partial.kind == Container::List
                    && partial.name == name
                    && partial.binding_owner == Some(owner)
            })
            .map(|_| definition)
    }

    pub(super) fn remember_partial_list(
        &mut self,
        target: &Expr,
        value: &Expr,
    ) -> Option<try_inputs::PartialFamily> {
        let Expr::Name(name) = target else {
            return None;
        };
        if name.id.as_str() == "_"
            || !matches!(value, Expr::List(list) if list.elts.is_empty())
            || self.partial_list_key(name.id.as_str()).is_some()
            || self.scopes.assigned_type(target).is_some()
        {
            return None;
        }
        let (owner, definition) = self.scopes.inferred_binding_identity(name.id.as_str())?;
        if definition != name.start() {
            return None;
        }
        let diagnostic = self.diagnostics.len();
        self.diagnostics
            .push(crate::infrastructure::diagnostics::at(
                self.source,
                QualityTool::Mypy,
                "var-annotated",
                format!(
                    "Need type annotation for \"{}\" (hint: \"{}: list[<type>] = ...\")",
                    name.id, name.id
                ),
                target.range(),
            ));
        self.partial_sets.insert(
            definition,
            partial_sets::PartialSet {
                name: name.id.to_string(),
                target: target.clone(),
                inferred: None,
                active: true,
                last_assignment: name.start(),
                preserve_error: false,
                diagnostic,
                owner: self.partial_set_scope,
                kind: Container::List,
                allow_unresolved_exit: self.partial_dict_dynamic,
                binding_owner: Some(owner),
            },
        );
        Some(try_inputs::PartialFamily::List)
    }

    pub(super) fn prepare_partial_list_assignment(
        &mut self,
        target: &Expr,
        value: &Expr,
        actual: Option<Type>,
    ) {
        let Expr::Name(name) = target else { return };
        let Some(key) = self.partial_list_key(name.id.as_str()) else {
            return;
        };
        if self.partial_sets[&key].inferred.is_some() {
            return;
        }
        if self.partial_list_unknown.contains(&key) {
            self.partial_sets.get_mut(&key).unwrap().active = false;
            return;
        }
        if self.flow.deleted.contains(name.id.as_str()) {
            self.mark_partial_list_unknown(key, target, "write after a deleted pending binding");
            self.partial_sets.get_mut(&key).unwrap().active = false;
            return;
        }
        if matches!(value, Expr::List(list) if list.elts.is_empty()) {
            let partial = self.partial_sets.get_mut(&key).unwrap();
            // A second incomplete assignment reads the existing partial lvalue.
            partial.preserve_error |= partial.last_assignment != name.start();
            partial.last_assignment = name.start();
            return;
        }
        let result = self.partial_list_assignment_type(value, actual);
        match result {
            Ok(ty) if self.resolve_partial_list(key, ty) => {
                self.partial_sets.get_mut(&key).unwrap().last_assignment = name.start();
            }
            Err(ListValidity::Unsupported(reason)) => {
                self.mark_partial_list_unknown(key, value, reason);
                self.partial_sets.get_mut(&key).unwrap().active = false;
            }
            Ok(_) => {
                self.mark_partial_list_unknown(
                    key,
                    target,
                    "binding identity changed during assignment",
                );
                self.partial_sets.get_mut(&key).unwrap().active = false;
            }
            Err(_) => {
                let partial = self.partial_sets.get_mut(&key).unwrap();
                partial.preserve_error = true;
                partial.active = false;
            }
        }
    }

    fn partial_list_assignment_type(
        &self,
        value: &Expr,
        actual: Option<Type>,
    ) -> Result<Type, ListValidity> {
        let ty = actual.ok_or(ListValidity::Unsupported("assignment type is unavailable"))?;
        if ty.is_any() {
            return self.partial_list_input_validity(value, ty).then_type(
                self.context
                    .intern(Node::Container(Container::List, vec![Type::Any])),
            );
        }
        let expanded = self.context.expanded_alias_instance(ty);
        if !matches!(self.context.node(expanded).as_deref(),
            Some(Node::Container(Container::List, arguments)) if arguments.len() == 1)
        {
            return Err(ListValidity::Invalid);
        }
        self.partial_list_input_validity(value, ty)
            .and(self.partial_list_validity(ty, true, &mut Default::default()))
            .then_type(ty)
    }

    pub(super) fn read_partial_list(&mut self, name: &ExprName) {
        if !(name.ctx.is_load() || name.ctx.is_del())
            || self.partial_list_lvalues.contains(&name.start())
            || self.flow.deleted.contains(name.id.as_str())
        {
            return;
        }
        if let Some(key) = self.partial_list_key(name.id.as_str()) {
            if !self.partial_list_unknown.contains(&key) {
                let partial = self.partial_sets.get_mut(&key).unwrap();
                partial.preserve_error |= partial.inferred.is_none();
            }
        }
    }

    pub(super) fn reassign_partial_list(&mut self, name: &ExprName) {
        let Some(key) = self.partial_list_key(name.id.as_str()) else {
            return;
        };
        if self.partial_sets[&key].last_assignment == name.start() {
            return;
        }
        if self.partial_sets[&key].inferred.is_none() && !self.partial_list_unknown.contains(&key) {
            self.mark_partial_list_unknown(
                key,
                name,
                "binding write bypasses direct assignment inference",
            );
        }
        self.partial_sets.get_mut(&key).unwrap().active = false;
    }

    pub(super) fn flag_partial_list_augmented(&mut self, assignment: &StmtAugAssign) {
        let Expr::Name(name) = assignment.target.as_ref() else {
            return;
        };
        if let Some(key) = self.partial_list_key(name.id.as_str()) {
            if self.partial_sets[&key].inferred.is_none() {
                if let Err(reason) = self.infer_partial_list_augmented(key, assignment) {
                    self.mark_partial_list_unknown(key, assignment.target.as_ref(), reason);
                }
            }
        }
    }

    fn resolve_partial_list(&mut self, key: TextSize, ty: Type) -> bool {
        if self.partial_list_unknown.contains(&key) {
            return false;
        }
        let partial = &self.partial_sets[&key];
        let name = partial.name.clone();
        if self.scopes.inferred_binding_identity(&name)
            != partial.binding_owner.map(|owner| (owner, key))
        {
            return false;
        }
        let Some(receipt) = self.scopes.refine_inferred_binding_receipt(&name, key, ty) else {
            return false;
        };
        self.partial_sets.get_mut(&key).unwrap().inferred = Some(ty);
        self.write_partial_declaration(&name, key, try_inputs::PartialFamily::List, receipt);
        true
    }

    fn pending_list_call<'a>(
        &self,
        expression: &'a Expr,
    ) -> Option<(TextSize, &'a ExprCall, &'a ExprName)> {
        let Expr::Call(call) = expression else {
            return None;
        };
        let Expr::Attribute(attribute) = call.func.as_ref() else {
            return None;
        };
        let Expr::Name(name) = attribute.value.as_ref() else {
            return None;
        };
        if !matches!(attribute.attr.as_str(), "append" | "extend")
            || call.arguments.args.len() != 1
            || call.arguments.args[0].is_starred_expr()
            || !call.arguments.keywords.is_empty()
            || self.flow.deleted.contains(name.id.as_str())
        {
            return None;
        }
        let key = self.partial_list_key(name.id.as_str())?;
        self.partial_sets[&key]
            .inferred
            .is_none()
            .then_some((key, call, name))
    }

    pub(super) fn visit_partial_list_call<'a>(&mut self, expression: &'a Expr) -> bool
    where
        Self: Visitor<'a>,
    {
        let Some((key, call, receiver)) = self.pending_list_call(expression) else {
            return false;
        };
        // A real argument read is committed before the receiver can be refined.
        self.visit_expr(&call.arguments.args[0]);
        if !self.partial_list_unknown.contains(&key) {
            match self.partial_list_call_type(call) {
                Ok(ty) if self.resolve_partial_list(key, ty) => {}
                Ok(_) => self.mark_partial_list_unknown(
                    key,
                    call,
                    "binding identity changed while visiting the argument",
                ),
                Err(ListValidity::Unsupported(reason)) => {
                    self.mark_partial_list_unknown(key, call, reason)
                }
                Err(_) => {}
            }
        }
        let uncertain = self.partial_list_unknown.contains(&key);
        if uncertain {
            // An unresolved receiver is not proved after unsupported inference.
            self.partial_list_lvalues.insert(receiver.start());
        }
        self.visit_expr(&call.func);
        if uncertain {
            self.partial_list_lvalues.remove(&receiver.start());
        }
        self.check_model_call(expression);
        self.check_function_call(expression);
        true
    }

    fn partial_list_call_type(&self, call: &ExprCall) -> Result<Type, ListValidity> {
        let Expr::Attribute(attribute) = call.func.as_ref() else {
            unreachable!()
        };
        let argument = &call.arguments.args[0];
        let ty = self
            .partial_list_observed_type(argument)
            .ok_or(ListValidity::Unsupported("argument type is unavailable"))?;
        let expanded = self.context.expanded_alias_instance(ty);
        let item = if attribute.attr.as_str() == "append" {
            self.partial_list_input_validity(argument, ty)
                .and(self.partial_list_validity(ty, true, &mut Default::default()))
                .then_type(ty)?
        } else if expanded.is_any() {
            self.partial_list_input_validity(argument, ty)
                .then_type(Type::Any)?
        } else {
            let node = self.context.node(expanded).ok_or(ListValidity::Invalid)?;
            let Node::Container(Container::List, arguments) = node.as_ref() else {
                return Err(ListValidity::Invalid);
            };
            let [item] = arguments.as_slice() else {
                return Err(ListValidity::Unsupported(
                    "list argument has an unsupported bound shape",
                ));
            };
            self.partial_list_input_validity(argument, ty)
                .and(self.partial_list_validity(*item, true, &mut Default::default()))
                .then_type(*item)?
        };
        Ok(self
            .context
            .intern(Node::Container(Container::List, vec![item])))
    }

    pub(super) fn partial_list_observed_type(&self, expression: &Expr) -> Option<Type> {
        self.observed_fstring_type(expression)
            .unwrap_or_else(|| self.infer(expression))
    }

    fn partial_list_input_validity(&self, expression: &Expr, ty: Type) -> ListValidity {
        if let Expr::Name(name) = expression {
            if self
                .partial_list_key(name.id.as_str())
                .is_some_and(|key| self.partial_list_unknown.contains(&key))
            {
                return ListValidity::Unsupported(
                    "argument reads a pending-list origin with unknown inference",
                );
            }
        }
        if matches!(expression, Expr::List(list) if list.elts.is_empty())
            || matches!(expression, Expr::Dict(dictionary) if dictionary.items.is_empty())
        {
            return ListValidity::Invalid;
        }
        self.partial_list_empty_flow(expression, ty)
    }

    // This only detects actual empty-bound producer routes. It does not infer types.
    // A result with no such route uses the existing graph predicate below.
    fn partial_list_empty_flow(&self, expression: &Expr, ty: Type) -> ListValidity {
        let inspect = |value: &Expr| {
            self.partial_list_observed_type(value).map_or(
                ListValidity::Unsupported("empty-bound child type is unavailable"),
                |ty| self.partial_list_input_validity(value, ty),
            )
        };
        let uncertain = |values: Vec<&Expr>| {
            let mut loss = false;
            for value in values {
                if !matches!(inspect(value), ListValidity::Valid) {
                    loss = true;
                }
            }
            if loss && self.partial_list_contains_any(ty, &mut Default::default()) {
                ListValidity::Unsupported("compound result lost an empty or unresolved bound")
            } else {
                ListValidity::Valid
            }
        };
        match expression {
            Expr::List(list) if list.elts.len() == 1 && !list.elts[0].is_starred_expr() => {
                // No join can remove the only element's missing bound.
                inspect(&list.elts[0])
            }
            Expr::Tuple(tuple) if tuple.elts.iter().all(|value| !value.is_starred_expr()) => tuple
                .elts
                .iter()
                .fold(ListValidity::Valid, |result, value| {
                    result.and(inspect(value))
                }),
            Expr::List(list) => {
                if let Some(Node::Container(Container::List, arguments)) =
                    self.context.node(ty).as_deref()
                {
                    if arguments.len() == 1
                        && arguments[0].is_any()
                        && list.elts.iter().any(|value| {
                            self.partial_list_observed_type(value)
                                .is_some_and(Type::is_any)
                        })
                    {
                        return ListValidity::Valid;
                    }
                }
                uncertain(list.elts.iter().collect())
            }
            Expr::Set(set) => uncertain(set.elts.iter().collect()),
            Expr::Dict(dictionary) => uncertain(
                dictionary
                    .items
                    .iter()
                    .flat_map(|item| item.key.iter().chain(std::iter::once(&item.value)))
                    .collect(),
            ),
            Expr::If(conditional) => uncertain(vec![&conditional.body, &conditional.orelse]),
            Expr::BoolOp(boolean) => uncertain(boolean.values.iter().collect()),
            Expr::Named(named) => inspect(&named.value),
            Expr::BinOp(binary) => uncertain(vec![&binary.left, &binary.right]),
            Expr::Subscript(subscript) => {
                if !matches!(inspect(&subscript.value), ListValidity::Valid) {
                    ListValidity::Unsupported("indexed result lost an empty or unresolved bound")
                } else {
                    ListValidity::Valid
                }
            }
            Expr::Call(call) => {
                let Some(signature) = self
                    .scopes
                    .callable_expression_in(&call.func, &self.environment())
                else {
                    return ListValidity::Valid;
                };
                let mut material = Vec::new();
                if let Some(returns) = signature.returns {
                    self.context.collect_variables(returns, &mut material);
                }
                if material
                    .iter()
                    .all(|variable| !signature.generic.contains(variable))
                {
                    return ListValidity::Valid;
                }
                material.retain(|variable| signature.generic.contains(variable));
                let feeds_return = |parameter: &signatures::Parameter| {
                    let mut variables = Vec::new();
                    if let Some(ty) = parameter.ty {
                        self.context.collect_variables(ty, &mut variables);
                    }
                    variables.iter().any(|variable| material.contains(variable))
                };
                let positional: Vec<_> = signature
                    .parameters
                    .iter()
                    .filter(|parameter| {
                        matches!(
                            parameter.kind,
                            signatures::Kind::PositionalOnly
                                | signatures::Kind::PositionalOrKeyword
                        )
                    })
                    .collect();
                let variadic = signature
                    .parameters
                    .iter()
                    .find(|parameter| parameter.kind == signatures::Kind::VariadicPositional);
                let keywords = signature
                    .parameters
                    .iter()
                    .find(|parameter| parameter.kind == signatures::Kind::VariadicKeyword);
                let mut values = Vec::new();
                for (index, value) in call.arguments.args.iter().enumerate() {
                    if positional
                        .get(index)
                        .copied()
                        .or(variadic)
                        .is_some_and(feeds_return)
                    {
                        values.push(value);
                    }
                }
                for keyword in &call.arguments.keywords {
                    let parameter = keyword
                        .arg
                        .as_ref()
                        .and_then(|name| {
                            signature.parameters.iter().find(|parameter| {
                                parameter.name == name.as_str()
                                    && matches!(
                                        parameter.kind,
                                        signatures::Kind::PositionalOrKeyword
                                            | signatures::Kind::KeywordOnly
                                    )
                            })
                        })
                        .or(keywords);
                    if parameter.is_some_and(feeds_return) {
                        values.push(&keyword.value);
                    }
                }
                uncertain(values)
            }
            Expr::Lambda(lambda) => uncertain(vec![&lambda.body]),
            Expr::ListComp(comprehension) => uncertain(
                std::iter::once(comprehension.elt.as_ref())
                    .chain(
                        comprehension
                            .generators
                            .iter()
                            .map(|generator| -> &Expr { &generator.iter }),
                    )
                    .collect(),
            ),
            Expr::SetComp(comprehension) => uncertain(
                std::iter::once(comprehension.elt.as_ref())
                    .chain(
                        comprehension
                            .generators
                            .iter()
                            .map(|generator| -> &Expr { &generator.iter }),
                    )
                    .collect(),
            ),
            Expr::Generator(comprehension) => uncertain(
                std::iter::once(comprehension.elt.as_ref())
                    .chain(
                        comprehension
                            .generators
                            .iter()
                            .map(|generator| -> &Expr { &generator.iter }),
                    )
                    .collect(),
            ),
            Expr::DictComp(comprehension) => uncertain(
                comprehension
                    .key
                    .iter()
                    .map(|key| key.as_ref())
                    .chain(std::iter::once(comprehension.value.as_ref()))
                    .chain(
                        comprehension
                            .generators
                            .iter()
                            .map(|generator| -> &Expr { &generator.iter }),
                    )
                    .collect(),
            ),
            _ => ListValidity::Valid,
        }
    }

    fn partial_list_contains_any(
        &self,
        ty: Type,
        seen: &mut std::collections::BTreeSet<Type>,
    ) -> bool {
        if ty.is_any() {
            return true;
        }
        if !seen.insert(ty) {
            return false;
        }
        match self.context.node(ty).as_deref() {
            Some(
                Node::Container(_, items)
                | Node::Tuple(items)
                | Node::Union(items)
                | Node::AppliedClass {
                    arguments: items, ..
                },
            ) => items
                .iter()
                .any(|&item| self.partial_list_contains_any(item, seen)),
            Some(Node::VariadicTuple(item) | Node::Unpack(item)) => {
                self.partial_list_contains_any(*item, seen)
            }
            Some(Node::InferredAlias { target, .. }) => {
                self.partial_list_contains_any(*target, seen)
            }
            Some(Node::AliasInstance(_, arguments)) => {
                arguments
                    .iter()
                    .any(|&item| self.partial_list_contains_any(item, seen))
                    || {
                        let expanded = self.context.expanded_alias_instance(ty);
                        expanded != ty && self.partial_list_contains_any(expanded, seen)
                    }
            }
            Some(Node::Callable(signature)) => {
                signature
                    .returns
                    .is_some_and(|returns| self.partial_list_contains_any(returns, seen))
                    || signature.parameters.iter().any(|parameter| {
                        parameter
                            .ty
                            .is_none_or(|ty| self.partial_list_contains_any(ty, seen))
                    })
                    || signature.overloads.iter().any(|variant| {
                        self.partial_list_contains_any(
                            self.context.callable_type(variant.clone()),
                            seen,
                        )
                    })
            }
            _ => false,
        }
    }

    fn partial_list_validity(
        &self,
        ty: Type,
        root: bool,
        seen: &mut std::collections::BTreeSet<Type>,
    ) -> ListValidity {
        if ty == Type::Never {
            return if root {
                ListValidity::Invalid
            } else {
                ListValidity::Unsupported("nested Never has no ambiguity evidence")
            };
        }
        if ty == Type::None && root {
            return ListValidity::Invalid;
        }
        if !seen.insert(ty) {
            return ListValidity::Valid;
        }
        let Some(node) = self.context.node(ty) else {
            return ListValidity::Valid;
        };
        if let Some((_, dictionary)) = self.context.typed_dict(ty) {
            return dictionary
                .items
                .iter()
                .fold(ListValidity::Valid, |result, item| {
                    result.and(self.partial_list_validity(item.ty, false, seen))
                });
        }
        let items = match node.as_ref() {
            Node::Container(_, items)
            | Node::Tuple(items)
            | Node::Union(items)
            | Node::AppliedClass {
                arguments: items, ..
            } => items.clone(),
            Node::VariadicTuple(item)
            | Node::Unpack(item)
            | Node::ParamSpecArgs(item)
            | Node::ParamSpecKwargs(item) => vec![*item],
            Node::Variable(variable) if variable.parameter_spec || variable.type_var_tuple => {
                std::iter::once(variable.bound)
                    .chain(variable.default)
                    .collect()
            }
            Node::Variable(_) => return ListValidity::Valid,
            Node::Callable(signature) => {
                let Some(returns) = signature.returns else {
                    return ListValidity::Unsupported("callable return type is unavailable");
                };
                let mut result = self.partial_list_validity(returns, false, seen);
                for parameter in signature.parameters.iter() {
                    result = result.and(self.partial_list_validity(
                        parameter.ty.unwrap_or(Type::Any),
                        false,
                        seen,
                    ));
                }
                for variant in signature.overloads.iter() {
                    result = result.and(self.partial_list_validity(
                        self.context.callable_type(variant.clone()),
                        false,
                        seen,
                    ));
                }
                return result;
            }
            Node::Parameters(parameters, _) => parameters
                .iter()
                .map(|parameter| parameter.ty.unwrap_or(Type::Any))
                .collect(),
            Node::AliasInstance(_, _) | Node::InferredAlias { .. } => {
                let expanded = self.context.expanded_alias_instance(ty);
                if expanded == ty {
                    return ListValidity::Unsupported("alias value has no finite queried type");
                }
                return self.partial_list_validity(expanded, root, seen);
            }
            Node::Alias(_, _) | Node::LiteralForm | Node::TypingForm(_) => {
                return ListValidity::Unsupported("value has an unrepresented inference shape")
            }
            Node::Deleted(_)
            | Node::Instance(_)
            | Node::Class(_)
            | Node::Literal(_)
            | Node::LiteralValue(_) => return ListValidity::Valid,
        };
        items.into_iter().fold(ListValidity::Valid, |result, item| {
            result.and(self.partial_list_validity(item, false, seen))
        })
    }

    fn mark_partial_list_unknown(&mut self, key: TextSize, expression: &impl Ranged, reason: &str) {
        let partial = &self.partial_sets[&key];
        let origin = crate::infrastructure::diagnostics::at(
            self.source,
            QualityTool::Mypy,
            "var-annotated",
            String::new(),
            partial.target.range(),
        )
        .location;
        let location = crate::infrastructure::diagnostics::at(
            self.source,
            QualityTool::Mypy,
            "var-annotated",
            String::new(),
            expression.range(),
        )
        .location;
        self.partial_list_gaps.insert(format!(
            "{}:{}:{}: pending list \"{}\" from {}:{} is unsupported: {}",
            self.source.path.display(),
            location.line,
            location.column,
            partial.name,
            origin.line,
            origin.column,
            reason,
        ));
        self.partial_list_unknown.insert(key);
    }

    pub(in crate::infrastructure::mypy) fn take_partial_list_gaps(&mut self) -> Vec<String> {
        std::mem::take(&mut self.partial_list_gaps)
            .into_iter()
            .collect()
    }
}

impl ListValidity {
    fn then_type(self, ty: Type) -> Result<Type, Self> {
        match self {
            Self::Valid => Ok(ty),
            other => Err(other),
        }
    }
}
