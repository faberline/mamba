//! Nominal indexing uses the same signatures and overload solver as method calls.
use super::*;
use mamba_language::application::quality_syntax::python_ast::{
    Arguments, ExprAttribute, ExprCall, ExprContext, ExprSlice, ExprSubscript,
};

impl Scopes {
    pub(in crate::infrastructure::mypy) fn nominal_subscript_call(
        &self,
        subscript: &ExprSubscript,
        environment: &BTreeMap<String, Type>,
    ) -> Option<ExprCall> {
        let receiver = self.subscript_receiver(subscript, environment)?;
        self.nominal_subscript_call_with_receiver(subscript, receiver)
    }

    pub(in crate::infrastructure::mypy) fn subscript_receiver(
        &self,
        subscript: &ExprSubscript,
        environment: &BTreeMap<String, Type>,
    ) -> Option<Type> {
        self.context
            .infer_with_calls(&subscript.value, environment, &|value| {
                self.expression_type(value, environment)
            })
    }

    pub(in crate::infrastructure::mypy) fn nominal_subscript_call_with_receiver(
        &self,
        subscript: &ExprSubscript,
        receiver: Type,
    ) -> Option<ExprCall> {
        if self.context.is_class_value(receiver)
            || self.context.typed_dict(receiver).is_some()
            || self.context.class_id(receiver).is_none()
        {
            return None;
        }
        self.class_member_signature(receiver, "__getitem__")?;
        Some(ExprCall {
            node_index: Default::default(),
            range_start: subscript.start(),
            func: Box::new(Expr::Attribute(ExprAttribute {
                node_index: Default::default(),
                range: subscript.value.range(),
                value: subscript.value.clone(),
                attr: mamba_language::application::quality_syntax::python_ast::Identifier::new(
                    "__getitem__",
                    subscript.value.range(),
                ),
                ctx: ExprContext::Load,
            })),
            arguments: Arguments {
                node_index: Default::default(),
                range: subscript.range(),
                args: vec![(*subscript.slice).clone()].into(),
                keywords: Default::default(),
            },
        })
    }

    pub(super) fn slice_expression_type(
        &self,
        slice: &ExprSlice,
        environment: &BTreeMap<String, Type>,
    ) -> Type {
        let arguments = [&slice.lower, &slice.upper, &slice.step]
            .into_iter()
            .map(|value| {
                value.as_deref().map_or(Type::None, |value| {
                    self.context
                        .infer_with_calls(value, environment, &|value| {
                            self.expression_type(value, environment)
                        })
                        .unwrap_or(Type::Any)
                })
            })
            .collect();
        let id = self.context.declare_class("builtins.slice", "slice", 0);
        self.context.applied_class(id, arguments, false)
    }
}
