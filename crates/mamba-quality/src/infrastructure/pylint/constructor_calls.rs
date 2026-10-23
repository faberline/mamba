//! Constructor delegation checks resolve direct bases and defining methods.
use super::{callables::Target, classes::Class, resolution::Context, Checker};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, Visitor},
    Expr, ExprAttribute, StmtFunctionDef,
};
use ruff_text_size::Ranged;
use std::{collections::BTreeSet, rc::Rc};

#[derive(Default)]
struct Calls<'a> {
    initializers: Vec<&'a ExprAttribute>,
}
impl<'a> Visitor<'a> for Calls<'a> {
    fn visit_expr(&mut self, expression: &'a Expr) {
        if let Expr::Call(call) = expression {
            if let Expr::Attribute(attribute) = call.func.as_ref() {
                if attribute.attr.as_str() == "__init__" {
                    self.initializers.push(attribute);
                }
            }
        }
        walk_expr(self, expression);
    }
}

impl Checker<'_> {
    pub(super) fn check_constructor_calls(&mut self, function: &StmtFunctionDef) {
        let Some((class, context)) = self.class_stack.last().cloned() else {
            return;
        };
        let caller = Context::Scope(self.scopes.current_index());
        let mut calls = Calls::default();
        calls.visit_body(&function.body);
        let bases: Vec<_> = class
            .bases
            .iter()
            .filter_map(|value| self.declared_base(value, context.clone()))
            .filter_map(|value| match value.target {
                Target::Class(class) => Some((class, value.context)),
                _ => None,
            })
            .collect();
        let mut pending = Vec::new();
        for (base, context) in &bases {
            let Some((owner, signature, origin)) =
                self.initializer_definition(base.clone(), context.clone())
            else {
                continue;
            };
            if (owner.identity.0 == "builtins" && matches!(owner.name.as_str(), "object" | "type"))
                || signature.pass_body
                || self.abstract_function(&signature, origin, true, Some(&owner))
            {
                continue;
            }
            pending.push((base.clone(), owner));
        }
        let mut called = BTreeSet::new();
        let mut owners = BTreeSet::new();
        for attribute in calls.initializers {
            if matches!(attribute.value.as_ref(), Expr::Call(call) if matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "super"))
            {
                return;
            }
            let Some(value) = self.expression_target_in(&attribute.value, caller.clone()) else {
                continue;
            };
            if matches!(value.target, Target::Super(_)) {
                return;
            }
            let target = match value.target {
                Target::Class(target) => target,
                Target::Instance(target, _) => {
                    self.emit(
                        "W0233",
                        "non-parent-init-called",
                        &format!(
                            "__init__ method from a non direct base class '{}' is called",
                            target.name
                        ),
                        attribute.range(),
                    );
                    continue;
                }
                _ => continue,
            };
            if let Some((_, owner)) = pending
                .iter()
                .find(|(base, _)| base.identity == target.identity)
            {
                called.insert(target.identity.clone());
                owners.insert(owner.identity.clone());
            } else if !bases
                .iter()
                .any(|(base, _)| base.identity == target.identity)
            {
                self.emit(
                    "W0233",
                    "non-parent-init-called",
                    &format!(
                        "__init__ method from a non direct base class '{}' is called",
                        target.name
                    ),
                    attribute.range(),
                );
            }
        }
        let overload = function.decorator_list.iter().any(|decorator| self.reference_in(super::values::path(&decorator.expression).unwrap_or_default(), context.clone()).is_some_and(|value| matches!(value.target, Target::Function(signature) if signature.name == "overload")));
        if overload {
            return;
        }
        for (base, owner) in pending {
            if called.contains(&base.identity) {
                continue;
            }
            if owners.contains(&owner.identity) {
                return;
            }
            if self
                .lineage(base.clone(), context.clone())
                .iter()
                .any(|(ancestor, _)| {
                    ancestor.name == "Protocol"
                        && matches!(ancestor.identity.0.as_str(), "typing" | "typing_extensions")
                })
            {
                return;
            }
            self.emit(
                "W0231",
                "super-init-not-called",
                &format!(
                    "__init__ method from base class '{}' is not called",
                    base.name
                ),
                self.declaration_range(function.name.range()),
            );
        }
    }

    fn initializer_definition(
        &self,
        class: Rc<Class>,
        context: Context,
    ) -> Option<(Rc<Class>, Rc<super::callables::Signature>, Context)> {
        for (class, context) in self.lineage(class, context) {
            if let Some(method) = class.methods.get("__init__") {
                return Some((class.clone(), method.signature.clone(), context));
            }
        }
        None
    }
}
