//! Context inference preserves call bindings, return alternatives, and generator origins.
use super::{
    callables::{Returns, Target},
    context_frames::Frame,
    context_values::Inferred,
    operand_inference::Operand,
    resolution::Context,
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::Expr;
use ruff_text_size::Ranged;
use std::{collections::BTreeSet, rc::Rc};

#[derive(Default)]
struct Active {
    nodes: BTreeSet<(Context, u32, u32, usize)>,
    calls: BTreeSet<(Context, u32)>,
}
impl Checker<'_> {
    pub(super) fn infer_context(&self, expression: &Expr, origin: Context) -> Inferred {
        self.context_value(expression, origin, None, &mut Active::default())
    }
    pub(super) fn infer_context_yield(&self, expression: &Expr) -> Option<Inferred> {
        let Expr::Call(call) = expression else {
            return None;
        };
        let origin = Context::Scope(self.scopes.current_index());
        let Inferred::Generator {
            signature,
            origin: producer,
            ..
        } = self.infer_context(expression, origin.clone())
        else {
            return None;
        };
        let function = self.source_function(&signature, &producer)?;
        let mut yields = super::context_yields::Yields::default();
        use mamba_language::application::quality_syntax::python_ast::visitor::Visitor;
        yields.visit_body(&function.body);
        let frame = self.context_frame(&signature, call, &origin, &producer, None);
        if frame.invalid || yields.0.is_empty() {
            return None;
        }
        Some(Inferred::join(
            yields
                .0
                .into_iter()
                .map(|value| {
                    self.context_value(
                        value,
                        producer.clone(),
                        Some(frame.clone()),
                        &mut Active::default(),
                    )
                })
                .collect(),
        ))
    }
    fn context_value(
        &self,
        expression: &Expr,
        origin: Context,
        frame: Option<Rc<Frame>>,
        active: &mut Active,
    ) -> Inferred {
        let key = (
            origin.clone(),
            expression.start().to_u32(),
            expression.end().to_u32(),
            frame.as_ref().map_or(0, |frame| Rc::as_ptr(frame) as usize),
        );
        if !active.nodes.insert(key.clone()) {
            return Inferred::unknown();
        }
        let result = self.context_step(expression, origin, frame, active);
        active.nodes.remove(&key);
        result
    }
    fn context_step(
        &self,
        expression: &Expr,
        origin: Context,
        frame: Option<Rc<Frame>>,
        active: &mut Active,
    ) -> Inferred {
        if let Expr::Name(name) = expression {
            if let Some(frame) = &frame {
                if let Some(value) = frame.locals.get(name.id.as_str()) {
                    return self.context_value(value, origin, Some(frame.clone()), active);
                }
                if let Some(value) = frame.arguments.get(name.id.as_str()) {
                    return self.context_value(
                        &value.expression,
                        value.origin.clone(),
                        value.frame.clone(),
                        active,
                    );
                }
            }
            if let Some((value, context)) = self.source_binding(name.id.as_str(), &origin) {
                if value.range() != expression.range() || context != origin {
                    return self.context_value(&value, context, frame, active);
                }
            }
        }
        if let Expr::If(value) = expression {
            let alternatives = match value.test.as_ref() {
                Expr::BooleanLiteral(test) => {
                    if test.value {
                        vec![&*value.body]
                    } else {
                        vec![&*value.orelse]
                    }
                }
                _ => vec![&*value.body, &*value.orelse],
            };
            return Inferred::join(
                alternatives
                    .into_iter()
                    .map(|value| self.context_value(value, origin.clone(), frame.clone(), active))
                    .collect(),
            );
        }
        if let Expr::Subscript(value) = expression {
            if let Some(node) = self.context_index_node(&value.value, &value.slice, origin.clone())
            {
                return self.context_value(&node, origin, frame, active);
            }
        }
        if let Expr::Call(call) = expression {
            let local = if let Expr::Name(name) = call.func.as_ref() {
                frame
                    .as_ref()
                    .and_then(|frame| frame.functions.get(name.id.as_str()))
                    .map(|function| super::resolution::Resolved {
                        target: Target::Function(function.clone()),
                        context: origin.clone(),
                    })
            } else {
                None
            };
            if let Some(resolved) =
                local.or_else(|| self.expression_target_in(&call.func, origin.clone()))
            {
                if let Target::Function(function) = resolved.target {
                    if function.generates {
                        return Inferred::Generator {
                            asynchronous: function.is_async,
                            name: function.name.clone(),
                            decorators: function
                                .decorators
                                .iter()
                                .filter_map(|value| {
                                    self.qualified_decorator(value, resolved.context.clone())
                                })
                                .collect(),
                            signature: function.clone(),
                            origin: resolved.context.clone(),
                            wrappers: Vec::new(),
                        };
                    }
                    let key = (
                        resolved.context.clone(),
                        function.definition_range.start().to_u32(),
                    );
                    if !active.calls.insert(key.clone()) {
                        return Inferred::unknown();
                    }
                    let inner =
                        self.context_frame(&function, call, &origin, &resolved.context, frame);
                    if inner.invalid {
                        active.calls.remove(&key);
                        return Inferred::unknown();
                    }
                    let values = function
                        .result_expressions
                        .iter()
                        .map(|value| {
                            self.context_value(
                                value,
                                resolved.context.clone(),
                                Some(inner.clone()),
                                active,
                            )
                        })
                        .collect::<Vec<_>>();
                    let mut result = if values.is_empty()
                        && matches!(function.returns, Returns::None | Returns::NoReturn)
                    {
                        Inferred::Operand(Operand::Literal("NoneType"))
                    } else {
                        Inferred::join(values)
                    };
                    if let Inferred::Generator { wrappers, .. } = &mut result {
                        wrappers.extend(function.decorators.iter().filter_map(|decorator| {
                            self.qualified_decorator(decorator, resolved.context.clone())
                        }));
                    }
                    active.calls.remove(&key);
                    return result;
                }
            }
        }
        if let Some(resolved) = self.expression_target_in(expression, origin.clone()) {
            if let Target::Function(signature) = resolved.target {
                return Inferred::Function {
                    bound: matches!(
                        signature.category,
                        "method" | "unbound method" | "classmethod"
                    ) && self.inferred_method_kind(
                        &signature.decorators,
                        resolved.context.clone(),
                        super::classes::MethodKind::Instance,
                    ) != super::classes::MethodKind::Property,
                    signature,
                    origin: resolved.context,
                };
            }
        }
        Inferred::Operand(self.operand_in(expression, origin, &mut Default::default()))
    }
    fn context_index_node(&self, collection: &Expr, index: &Expr, origin: Context) -> Option<Expr> {
        let values = match collection {
            Expr::Tuple(value) => &value.elts,
            Expr::List(value) => &value.elts,
            Expr::Dict(value) => {
                return value
                    .items
                    .iter()
                    .find(|item| {
                        item.key
                            .as_ref()
                            .is_some_and(|key| super::comparison_constants::same(key, index))
                    })
                    .map(|item| item.value.clone())
            }
            _ => return None,
        };
        let index = self.unpacking_integer(index, origin)?;
        let index = if index < 0 {
            i64::try_from(values.len()).ok()?.checked_add(index)?
        } else {
            index
        };
        values.get(usize::try_from(index).ok()?).cloned()
    }
}
