//! Object lookup binds receivers and retains each base declaration's context.
use super::{
    callables::{Returns, Signature, Target},
    classes::{Class, MethodKind},
    resolution::{Context, Resolved},
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::Expr;
use std::{collections::BTreeSet, rc::Rc};
mod builtin_containers;
mod builtin_scope;
mod builtin_values;
mod container_members;
pub(super) mod scope_members;
mod scope_values;

impl Checker<'_> {
    pub(super) fn declared_base(&self, expression: &Expr, context: Context) -> Option<Resolved> {
        let named_tuple = match expression {
            Expr::Name(name) => match &context { Context::Scope(index) => matches!(self.scopes.call_target_in(name.id.as_str(), *index), Some((_, Target::Imported(import))) if matches!(import.module.as_str(), "typing" | "typing_extensions") && import.member.as_deref() == Some("NamedTuple")), Context::Module(module) => matches!(self.modules.call_target(self.source.project, module, name.id.as_str()), Some(Target::Imported(import)) if matches!(import.module.as_str(), "typing" | "typing_extensions") && import.member.as_deref() == Some("NamedTuple")) },
            Expr::Attribute(value) if value.attr.as_str() == "NamedTuple" => self.expression_target_in(&value.value, context.clone()).is_some_and(|value| matches!(value.target, Target::Module(module) if matches!(module.as_str(), "typing" | "typing_extensions"))),
            _ => false,
        };
        if named_tuple {
            return Some(Resolved {
                target: Target::Class(self.modules.builtin_class("tuple")?),
                context: Context::Module("builtins".into()),
            });
        }
        self.expression_target_in(expression, context).or_else(|| {
            let Expr::Name(name) = expression else {
                return None;
            };
            Some(Resolved {
                target: Target::Class(self.modules.builtin_class(name.id.as_str())?),
                context: Context::Module("builtins".into()),
            })
        })
    }

    pub(super) fn inferred_base(
        &self,
        expression: &Expr,
        context: Context,
    ) -> Option<(Rc<Class>, Context)> {
        let resolved = self.declared_base(expression, context)?;
        match resolved.target {
            Target::Class(class) => Some((class, resolved.context)),
            Target::Instance(class, origin) if class.identity.0 != "enum" => Some((class, origin)),
            Target::Value(value) => Some((
                self.modules.builtin_class(value.builtin_name()?)?,
                Context::Module("builtins".into()),
            )),
            _ => None,
        }
    }

    pub(super) fn lineage(&self, class: Rc<Class>, context: Context) -> Vec<(Rc<Class>, Context)> {
        let mut pending = vec![(class, context)];
        let mut visited = BTreeSet::new();
        let mut order = Vec::new();
        while let Some((class, context)) = pending.pop() {
            if !visited.insert(class.identity.clone()) {
                continue;
            }
            for base in class.bases.iter().rev() {
                if let Some((base, context)) = self.inferred_base(base, context.clone()) {
                    pending.push((base, context));
                }
            }
            order.push((class, context));
        }
        order
    }

    pub(super) fn class_member(
        &self,
        class: Rc<Class>,
        origin: Context,
        name: &str,
        instance: bool,
    ) -> Option<Resolved> {
        if let Some(value) = self.builtin_class_scope_value(&class, name, instance) {
            return Some(value);
        }
        for (class, context) in self.lineage(class.clone(), origin.clone()) {
            if let Some(value) = self.local_class_member(&class, context, name, instance) {
                return Some(value);
            }
        }
        self.source_class_scope_value(class, origin, name, instance)
    }

    pub(super) fn local_class_member(
        &self,
        class: &Class,
        context: Context,
        name: &str,
        instance: bool,
    ) -> Option<Resolved> {
        if instance {
            if let Some(target) = class.attributes.get(name) {
                return Some(Resolved {
                    target: target.clone(),
                    context,
                });
            }
        }
        if let Some(method) = class.methods.get(name) {
            let kind = self.inferred_method_kind(
                &method.signature.decorators,
                context.clone(),
                method.kind,
            );
            let target = if kind == MethodKind::Property && instance {
                Target::Value(Rc::new(method.value.clone()))
            } else {
                let (implicit, category) = match kind {
                    MethodKind::Static => (0, "staticmethod"),
                    MethodKind::Class => (1, "classmethod"),
                    _ if instance => (1, "method"),
                    _ => (0, "unbound method"),
                };
                Target::Function(Rc::new(method.signature.bound(implicit, category)))
            };
            return Some(Resolved { target, context });
        }
        if let Some(target) = class.members.get(name) {
            return Some(Resolved {
                target: target.clone(),
                context,
            });
        }
        None
    }

    pub(super) fn constructor(&self, class: Rc<Class>, context: Context) -> Option<Rc<Signature>> {
        for (class, _) in self.lineage(class, context) {
            if let Some(method) = class.methods.get("__init__") {
                if matches!(class.identity.0.as_str(), "builtins" | "collections") {
                    return None;
                }
                let mut signature = method.signature.bound(1, "constructor");
                signature.returns = Returns::Value;
                return Some(Rc::new(signature));
            }
        }
        None
    }

    pub(super) fn dynamic_member(&self, class: Rc<Class>, context: Context) -> bool {
        self.lineage(class, context).iter().any(|(class, _)| {
            !(class.identity.0 == "builtins" && class.name == "object")
                && (class.methods.contains_key("__getattr__")
                    || class.methods.contains_key("__getattribute__"))
        })
    }
}
