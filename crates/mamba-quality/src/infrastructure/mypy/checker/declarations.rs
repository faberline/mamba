//! Function declarations enter separate lexical and control-flow state.
use super::super::classes::MethodKind;
use super::*;
use quality_syntax::python_ast::{StmtFunctionDef, TypeParams};
mod decorators;
mod type_comments;
mod variants;
pub(super) use type_comments::TypeComments;

impl Checker<'_> {
    pub(super) fn visit_function<'a>(&mut self, function: &'a StmtFunctionDef)
    where
        Self: Visitor<'a>,
    {
        if let Some(ty) = self.scopes.register_compound_function(function) {
            let target = Expr::Name(quality_syntax::python_ast::ExprName {
                node_index: Default::default(),
                range: function.name.range(),
                id: function.name.to_string().into(),
                ctx: quality_syntax::python_ast::ExprContext::Store,
            });
            self.flow_assignment(&target, Some(ty));
        }
        if self.scopes.is_overload(function) {
            if !signatures::no_type_check(function) {
                let implicit_receiver = self.in_class
                    && (self.current_method_kind(function) != MethodKind::Static
                        || function.name.as_str() == "__new__");
                self.check_missing_function_annotations(function, implicit_receiver);
            }
            return;
        }
        if self.scopes.import_redefinition(function.name.as_str()) {
            self.emit(
                "no-redef",
                format!(
                    "Name \"{}\" already defined (possibly by an import)",
                    function.name
                ),
                function,
            );
            return;
        }
        if signatures::no_type_check(function) {
            return;
        }
        self.check_property_decorators(function);
        self.check_computed_decorators(function);
        self.check_untyped_decorators(function);
        self.check_dataclass_transform_function(function);
        self.check_dataclass_order_declaration(function);
        self.check_type_default_version(function.type_params.as_deref(), function);
        self.check_parameter_spec_defaults(function.type_params.as_deref());
        self.check_tuple_variable_defaults(function.type_params.as_deref());
        let protocol_stub = self.protocol_stub(function);
        if self.in_class
            && !self
                .scopes
                .callable(function.name.as_str())
                .is_some_and(|signature| !signature.overloads.is_empty())
        {
            self.check_method_override(function);
        }
        let method_kind = self.in_class.then(|| self.current_method_kind(function));
        let signature = self
            .scopes
            .function_declaration_signature(function)
            .map(|signature| {
                self.scopes.capture_function_variables(std::sync::Arc::new(
                    self.scopes
                        .refresh_function_annotations(function, &signature),
                ))
            })
            .unwrap_or_else(|| {
                let signature = signatures::Signature::from_function(
                    function,
                    &self.context,
                    &self.scopes.variables(),
                    &self.scopes.function_binder_input(
                        function,
                        &self
                            .class_types
                            .last()
                            .copied()
                            .flatten()
                            .and_then(|ty| self.context.class_id(ty))
                            .map(|id| self.context.class(id).generic.to_vec())
                            .unwrap_or_default(),
                    ),
                );
                std::sync::Arc::new(
                    self.scopes
                        .refresh_function_annotations(function, &signature),
                )
            });
        let annotation_variables = self.scopes.annotation_variables_in(&signature.variables);
        self.check_new_return(function, &signature);
        let mut annotation_bound: std::collections::BTreeSet<_> =
            signature.generic.iter().copied().collect();
        for &ty in signature.variables.values() {
            if self.scopes.type_variable_bound(ty) {
                annotation_bound.insert(ty);
            }
        }
        let previous_annotation_scope = self
            .scopes
            .annotation_scope(annotation_variables.clone(), annotation_bound);
        let previous_signature = std::mem::replace(&mut self.annotation_in_signature, true);
        let previous_errors = std::mem::take(&mut self.parameter_spec_errors);
        self.check_parameter_spec_function(function);
        for parameter in function.parameters.iter() {
            if let Some(annotation) = parameter.annotation() {
                self.annotation_allow_component = parameter.is_variadic();
                self.check_callable_annotation(annotation);
                self.annotation_allow_component = false;
            }
        }
        if let Some(annotation) = function.returns.as_deref() {
            let _returned_callable_validation = self.scopes.modules.as_ref().and_then(|modules| {
                self.context.returned_callable_validation(
                    function,
                    signature.returns,
                    &annotation_variables,
                    &modules.namespace(),
                    &self.source.sha256,
                    self.python_target,
                )
            });
            self.check_callable_annotation(annotation);
        }
        self.annotation_in_signature = previous_signature;
        self.parameter_spec_errors = previous_errors;
        self.scopes
            .annotation_scope(previous_annotation_scope.0, previous_annotation_scope.1);
        let implicit_receiver = method_kind.is_some_and(|kind| kind != MethodKind::Static)
            || (self.in_class && function.name.as_str() == "__new__");
        self.check_predicate_declaration(function, &signature, implicit_receiver);
        self.check_function_variance(function, implicit_receiver, &signature.variables);
        self.check_return_variable(function, &signature);
        self.check_overload_implementation(function, &signature);
        let inferred_initializer =
            self.check_missing_function_annotations(function, implicit_receiver);
        let environment = function
            .parameters
            .iter()
            .enumerate()
            .map(|(index, parameter)| {
                (
                    parameter.name().to_string(),
                    if parameter.is_variadic() {
                        let item = match parameter.annotation() {
                            Some(annotation) => self
                                .context
                                .annotation_with_variables(annotation, &annotation_variables),
                            None => Some(types::Type::Any),
                        };
                        item.map(|item| {
                            if self.context.unpack_target(item).is_some() {
                                return self.context.normalize_tuple(vec![item]);
                            }
                            if self.context.node(item).is_some_and(|node| {
                                matches!(
                                    node.as_ref(),
                                    types::Node::ParamSpecArgs(_) | types::Node::ParamSpecKwargs(_)
                                )
                            }) {
                                return item;
                            }
                            let positional = function
                                .parameters
                                .vararg
                                .as_ref()
                                .is_some_and(|argument| &argument.name == parameter.name());
                            self.context.intern(if positional {
                                types::Node::VariadicTuple(item)
                            } else {
                                types::Node::Container(
                                    types::Container::Dict,
                                    vec![types::Type::Str, item],
                                )
                            })
                        })
                    } else {
                        match parameter.annotation() {
                            Some(annotation) => self
                                .context
                                .annotation_with_variables(annotation, &annotation_variables),
                            None if implicit_receiver && index == 0 => signature
                                .parameters
                                .first()
                                .and_then(|parameter| parameter.ty)
                                .filter(|&ty| {
                                    let instance = self.context.instance_type(ty).unwrap_or(ty);
                                    self.context.is_source_self_variable(instance)
                                })
                                .or_else(|| {
                                    self.class_types.last().copied().flatten().and_then(|ty| {
                                        if method_kind == Some(MethodKind::Class)
                                            || function.name.as_str() == "__new__"
                                        {
                                            Some(ty)
                                        } else {
                                            self.context.instance_type(ty)
                                        }
                                    })
                                }),
                            None => Some(types::Type::Any),
                        }
                    },
                )
            })
            .collect();
        let returns = function
            .returns
            .as_deref()
            .and_then(|expression| {
                self.context
                    .annotation_with_variables(expression, &annotation_variables)
            })
            .or_else(|| inferred_initializer.then_some(types::Type::None));
        self.check_generator_declaration(function, returns);
        self.check_function_variants(
            function,
            &signature,
            environment,
            returns,
            implicit_receiver,
            protocol_stub,
        );
    }

    fn check_missing_function_annotations(
        &mut self,
        function: &StmtFunctionDef,
        implicit_receiver: bool,
    ) -> bool {
        let annotations =
            self.type_comments
                .annotations(function, !self.classes.is_empty(), &self.source.text);
        let any_annotation =
            annotations.returns || annotations.parameters.iter().any(|&present| present);
        let initializer =
            self.in_class && matches!(function.name.as_str(), "__init__" | "__init_subclass__");
        let inferred_initializer =
            initializer && annotations.parameters.iter().any(|&present| present);
        let range = self.function_range(function);
        if !any_annotation {
            let no_inputs = function.parameters.is_empty()
                || (function.parameters.len() == 1
                    && function.parameters.posonlyargs.is_empty()
                    && !signatures::special_methods::positional_only(function.name.as_str())
                    && function.parameters.iter().next().is_some_and(|parameter| {
                        matches!(parameter.name().as_str(), "self" | "cls")
                    }));
            if no_inputs {
                let mut returns = NontrivialReturns::default();
                // FuncItem visits defaults and body, not the root's own decorators.
                returns.visit_parameters(&function.parameters);
                returns.visit_body(&function.body);
                let hint = (!returns.0 && !signatures::Signature::is_generator(function))
                    .then(|| "Use \"-> None\" if function does not return a value".into());
                self.emit_with_hint(
                    "no-untyped-def",
                    "Function is missing a return type annotation".into(),
                    &range,
                    hint,
                );
            } else {
                self.emit(
                    "no-untyped-def",
                    "Function is missing a type annotation".into(),
                    &range,
                );
            }
        } else {
            if !annotations.returns && !inferred_initializer {
                self.emit(
                    "no-untyped-def",
                    "Function is missing a return type annotation".into(),
                    &range,
                );
            }
            if annotations
                .parameters
                .iter()
                .enumerate()
                .any(|(index, &present)| !present && (!implicit_receiver || index != 0))
            {
                self.emit(
                    "no-untyped-def",
                    "Function is missing a type annotation for one or more parameters".into(),
                    &range,
                );
            }
        }
        inferred_initializer
    }

    fn check_return_variable(
        &mut self,
        function: &StmtFunctionDef,
        signature: &signatures::Signature,
    ) {
        let returns = if function.is_async {
            signature
                .returns
                .and_then(|ty| self.context.coroutine_result(ty))
        } else {
            signature.returns
        };
        let Some(returns) = returns else {
            return;
        };
        if self.scopes.type_variable_bound(returns) {
            return;
        }
        if self.in_class
            && self
                .class_types
                .last()
                .copied()
                .flatten()
                .and_then(|ty| self.context.class_id(ty))
                .is_some_and(|id| self.context.class(id).generic.contains(&returns))
        {
            return;
        }
        if !self
            .context
            .node(returns)
            .is_some_and(|node| matches!(node.as_ref(), types::Node::Variable(_)))
        {
            return;
        }
        let mut inputs = Vec::new();
        for parameter in signature.parameters.iter() {
            if let Some(ty) = parameter.ty {
                self.context.collect_variables(ty, &mut inputs);
            }
        }
        if !inputs.contains(&returns) {
            if let Some(annotation) = function.returns.as_deref() {
                self.emit(
                    "type-var",
                    "A function returning TypeVar should receive at least one argument containing the same TypeVar".into(),
                    &ruff_text_size::TextRange::at(annotation.start(), ruff_text_size::TextSize::new(1)),
                );
            }
        }
    }
}

#[derive(Default)]
struct NontrivialReturns(bool);

impl<'a> Visitor<'a> for NontrivialReturns {
    fn visit_annotation(&mut self, _annotation: &'a Expr) {}

    fn visit_type_params(&mut self, _parameters: &'a TypeParams) {}

    fn visit_expr(&mut self, expression: &'a Expr) {
        if let Expr::Lambda(lambda) = expression {
            if let Some(parameters) = &lambda.parameters {
                self.visit_parameters(parameters);
            }
            // Mypy's LambdaExpr body is a synthetic ReturnStmt. Its visitor does
            // not recurse into the returned expression after classifying it.
            if !matches!(lambda.body.as_ref(), Expr::NoneLiteral(_)) {
                self.0 = true;
            }
        } else {
            walk_expr(self, expression);
        }
    }

    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if let Stmt::Return(statement) = statement {
            if !matches!(
                statement.value.as_deref(),
                None | Some(Expr::NoneLiteral(_))
            ) {
                self.0 = true;
            }
        } else {
            // The pinned ReturnSeeker also walks nested function and class bodies.
            walk_stmt(self, statement);
        }
    }
}
