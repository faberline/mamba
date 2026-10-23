//! Synchronous with targets retain the enclosing lexical scope.
use super::flow::WithTempKey;
use super::*;
use quality_syntax::python_ast::{
    Arguments, ExprAttribute, ExprCall, ExprContext, ExprName, Identifier, StmtAssign, StmtWith,
};

impl Checker<'_> {
    pub(super) fn check_with<'a>(&mut self, with: &'a StmtWith)
    where
        Self: Visitor<'a>,
    {
        for item in &with.items {
            self.visit_expr(&item.context_expr);
            // Capture the manager before an `as` target can rebind its name.
            let context = self.infer(&item.context_expr);
            let context_type = context.unwrap_or(types::Type::Any);
            let entered = Self::with_method_call(&item.context_expr, "__enter__", Vec::new());
            let enter_call = self.check_with_method(&entered);
            let entered_type = self.infer(&entered);
            if let Some(target) = &item.optional_vars {
                self.visit_expr(target);
                if let Some(ty) = entered_type {
                    self.assign_with_result(target, ty, item.context_expr.start());
                } else {
                    self.bind_unknown_with_target(target);
                }
            }
            // Mypy's protocol uses three Any TempNodes, rather than three None values.
            let previous_context = self
                .flow
                .narrowed
                .replace_with_temp(WithTempKey::ExitContext, Some(context_type));
            let previous_argument = self
                .flow
                .narrowed
                .replace_with_temp(WithTempKey::ExitArgument, Some(types::Type::Any));
            let receiver = Expr::Name(ExprName {
                node_index: Default::default(),
                range: item.context_expr.range(),
                id: WithTempKey::ExitContext.name().into(),
                ctx: ExprContext::Load,
            });
            let arguments = (0..3)
                .map(|_| {
                    Expr::Name(ExprName {
                        node_index: Default::default(),
                        range: ruff_text_size::TextRange::empty(item.context_expr.start()),
                        id: WithTempKey::ExitArgument.name().into(),
                        ctx: ExprContext::Load,
                    })
                })
                .collect();
            let exited = Self::with_method_call(&receiver, "__exit__", arguments);
            let exit_call = self.check_with_method(&exited);
            self.flow
                .narrowed
                .replace_with_temp(WithTempKey::ExitContext, previous_context);
            self.flow
                .narrowed
                .replace_with_temp(WithTempKey::ExitArgument, previous_argument);
            let proved = self.try_with_item_active(with, &item.context_expr)
                && context.is_some_and(|receiver| {
                    matches!(
                        self.context.node(receiver).as_deref(),
                        Some(
                            types::Node::Instance(_)
                                | types::Node::AppliedClass { value: false, .. }
                        )
                    ) && enter_call.as_ref().is_some_and(|(actual, signature)| {
                        *actual == receiver
                            && signature.returns == entered_type
                            && self.flat_with_call(signature)
                    }) && exit_call.as_ref().is_some_and(|(actual, signature)| {
                        *actual == receiver
                            && self.flat_with_call(signature)
                            && signature.returns.is_some_and(|returned| {
                                returned == types::Type::None
                                    || matches!(
                                        self.context.node(returned).as_deref(),
                                        Some(
                                            types::Node::Literal(types::Literal::Bool(false))
                                                | types::Node::LiteralValue(types::Literal::Bool(
                                                    false
                                                ))
                                        )
                                    )
                            })
                    })
                });
            self.complete_try_with_item(with, &item.context_expr, proved);
        }
        // Match the existing with walker; do not create or leave a scope.
        self.visit_body(&with.body);
    }

    fn with_method_call(context: &Expr, method: &str, arguments: Vec<Expr>) -> Expr {
        let point = ruff_text_size::TextRange::empty(context.start());
        Expr::Call(ExprCall {
            node_index: Default::default(),
            range_start: context.start(),
            func: Box::new(Expr::Attribute(ExprAttribute {
                node_index: Default::default(),
                range: context.range(),
                value: Box::new(context.clone()),
                attr: Identifier::new(method, point),
                ctx: ExprContext::Load,
            })),
            arguments: Arguments {
                range: context.range(),
                node_index: Default::default(),
                args: arguments.into(),
                keywords: Default::default(),
            },
        })
    }

    fn check_with_method(
        &mut self,
        expression: &Expr,
    ) -> Option<(types::Type, std::sync::Arc<signatures::Signature>)> {
        let Expr::Call(call) = expression else {
            unreachable!("with_method_call constructs a call");
        };
        let Expr::Attribute(attribute) = call.func.as_ref() else {
            unreachable!("with_method_call constructs an attribute");
        };
        let receiver = self.check_object_attribute_binding(attribute);
        // Neither the enter result nor the ignored exit result requires a value.
        let previous_none = self.none_allowed.replace(call.range());
        let selected = self.check_with_protocol_call(expression);
        self.none_allowed = previous_none;
        receiver
            .zip(selected)
            .and_then(|((ty, bound), selected)| (bound == selected).then_some((ty, selected)))
    }

    fn flat_with_call(&self, signature: &signatures::Signature) -> bool {
        !signature.implicit
            && !signature.ignored
            && !signature.constructor
            && !signature.ellipsis
            && signature.narrowing.is_none()
            && signature.generic.is_empty()
            && signature.overloads.is_empty()
            && signature.bound_receiver.is_some()
            && signature.return_explicit
            && signature
                .returns
                .is_some_and(|ty| self.known_with_type(ty, 0))
            && signature.parameters.iter().all(|parameter| {
                matches!(
                    parameter.kind,
                    signatures::Kind::PositionalOnly | signatures::Kind::PositionalOrKeyword
                ) && parameter.ty.is_some_and(|ty| self.known_with_type(ty, 0))
            })
    }

    // Read only already-selected types. No alias expansion or metadata binding.
    fn known_with_type(&self, ty: types::Type, depth: usize) -> bool {
        if depth > 8 {
            return false;
        }
        match ty {
            types::Type::Any | types::Type::AnySpecial | types::Type::Never => false,
            types::Type::Node(_) => match self.context.node(ty).as_deref() {
                Some(types::Node::Instance(_)) => true,
                Some(types::Node::AppliedClass {
                    arguments,
                    value: false,
                    ..
                }) => arguments
                    .iter()
                    .all(|&ty| self.known_with_type(ty, depth + 1)),
                Some(types::Node::Container(types::Container::TypeObject, items))
                    if items.len() == 1 =>
                {
                    self.known_with_type(items[0], depth + 1)
                }
                Some(types::Node::Union(items)) if !items.is_empty() => {
                    items.iter().all(|&ty| self.known_with_type(ty, depth + 1))
                }
                Some(types::Node::Literal(_) | types::Node::LiteralValue(_)) => true,
                _ => false,
            },
            _ => true,
        }
    }

    fn assign_with_result(
        &mut self,
        target: &Expr,
        entered: types::Type,
        start: ruff_text_size::TextSize,
    ) {
        // A private typed value plays the role of Mypy's TempNode.
        // Assignment checks must not re-infer the context after a target changes it.
        let previous = self
            .flow
            .narrowed
            .replace_with_temp(WithTempKey::EnterResult, Some(entered));
        let assignment = StmtAssign {
            node_index: Default::default(),
            range: target.range(),
            targets: vec![target.clone()],
            value: Box::new(Expr::Name(ExprName {
                node_index: Default::default(),
                range: ruff_text_size::TextRange::empty(start),
                id: WithTempKey::EnterResult.name().into(),
                ctx: ExprContext::Load,
            })),
        };
        let first = self.diagnostics.len();
        self.check_assignment(&assignment);
        for diagnostic in &mut self.diagnostics[first..] {
            if diagnostic.rule == "assignment"
                && diagnostic.location.offset == Some(start.to_u32())
                && diagnostic.location.end_line.is_none()
                && diagnostic.location.end_column.is_none()
                && diagnostic.location.end_offset.is_none()
            {
                // A TempNode point retains its end position without a byte span.
                diagnostic.location.end_line = Some(diagnostic.location.line);
                diagnostic.location.end_column = Some(i64::from(diagnostic.location.column));
            }
        }
        self.flow
            .narrowed
            .replace_with_temp(WithTempKey::EnterResult, previous);
    }

    fn bind_unknown_with_target(&mut self, target: &Expr) {
        match target {
            Expr::Name(_) => {
                self.scopes.bind(target, None);
                self.flow_assignment(target, None);
            }
            Expr::Tuple(tuple) => {
                for target in &tuple.elts {
                    self.bind_unknown_with_target(target);
                }
            }
            Expr::List(list) => {
                for target in &list.elts {
                    self.bind_unknown_with_target(target);
                }
            }
            Expr::Starred(starred) => self.bind_unknown_with_target(&starred.value),
            _ => {}
        }
    }
}
