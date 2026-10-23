//! A super proxy retains its MRO pointer and bound receiver without execution.
use super::{
    callables::Target,
    classes::{Class, MethodKind},
    resolution::{Context, Resolved},
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::{Expr, ExprCall};
use ruff_text_size::Ranged;
use std::rc::Rc;

#[derive(Clone)]
pub(super) struct Proxy {
    pub class: Rc<Class>,
    pub context: Context,
    pub pointer: (String, u32),
    pub name: String,
    pub instance: bool,
}

impl Checker<'_> {
    pub(super) fn super_proxy(&self, call: &ExprCall, context: Context) -> Option<Rc<Proxy>> {
        if !matches!(
            self.method_kind,
            Some(MethodKind::Instance | MethodKind::Class | MethodKind::Property)
        ) || call.arguments.args.len() == 1
        {
            return None;
        }
        let (current, origin) = self.class_stack.last()?.clone();
        let (pointer, name) = if let Some(first) = call.arguments.args.first() {
            let value = self.expression_target_in(first, context.clone())?;
            let Target::Class(class) = value.target else {
                return None;
            };
            (class.identity.clone(), class.name.clone())
        } else {
            (current.identity.clone(), current.name.clone())
        };
        let (class, origin, instance) = if let Some(receiver) = call.arguments.args.get(1) {
            let value = self.expression_target_in(receiver, context)?;
            match value.target {
                Target::Class(class) => (class, value.context, false),
                Target::Instance(class, origin) => (class, origin, true),
                _ => return None,
            }
        } else if call.arguments.args.is_empty() {
            (current, origin, self.method_kind != Some(MethodKind::Class))
        } else {
            return None;
        };
        if self
            .mro(class.clone(), origin.clone())
            .is_ok_and(|order| order.iter().all(|(class, _)| class.identity != pointer))
        {
            return None;
        }
        Some(Rc::new(Proxy {
            class,
            context: origin,
            pointer,
            name,
            instance,
        }))
    }

    pub(super) fn super_member(&self, proxy: &Proxy, name: &str) -> Option<Resolved> {
        let order = self.mro(proxy.class.clone(), proxy.context.clone()).ok()?;
        let offset = order
            .iter()
            .position(|(class, _)| class.identity == proxy.pointer)?;
        for (class, context) in order.into_iter().skip(offset + 1) {
            if let Some(method) = class.methods.get(name) {
                let kind = self.inferred_method_kind(
                    &method.signature.decorators,
                    context.clone(),
                    method.kind,
                );
                let target = match kind {
                    MethodKind::Property if proxy.instance => {
                        Target::Value(Rc::new(method.value.clone()))
                    }
                    MethodKind::Property => {
                        Target::Function(Rc::new(method.signature.bound(0, "property")))
                    }
                    MethodKind::Static => {
                        Target::Function(Rc::new(method.signature.bound(0, "staticmethod")))
                    }
                    MethodKind::Class => {
                        Target::Function(Rc::new(method.signature.bound(1, "classmethod")))
                    }
                    MethodKind::Instance => {
                        Target::Function(Rc::new(method.signature.bound(1, "method")))
                    }
                };
                return Some(Resolved { target, context });
            }
            if let Some(target) = class.members.get(name) {
                return Some(Resolved {
                    target: target.clone(),
                    context,
                });
            }
        }
        None
    }

    pub(super) fn check_super_arguments_style(&mut self, call: &ExprCall) {
        let [Expr::Name(class), Expr::Name(receiver)] = call.arguments.args.as_ref() else {
            return;
        };
        if matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "super")
            && receiver.id.as_str() == "self"
            && self
                .class_stack
                .last()
                .is_some_and(|(current, _)| current.name == class.id.as_str())
        {
            self.emit(
                "R1725",
                "super-with-arguments",
                "Consider using Python 3 style super() without arguments",
                call.range(),
            );
        }
    }
}
