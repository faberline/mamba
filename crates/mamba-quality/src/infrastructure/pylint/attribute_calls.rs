//! Literal builtin setattr calls retain method and shadowing gates for W0201.
use super::{
    classes::{AttributeDefinition, Class, MethodKind},
    resolution::Context,
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    Expr, ExprCall, Stmt,
};
use ruff_text_size::Ranged;
use std::{collections::BTreeSet, rc::Rc};

#[derive(Default)]
struct Calls<'a>(Vec<&'a ExprCall>);
impl<'a> Visitor<'a> for Calls<'a> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if !matches!(statement, Stmt::FunctionDef(_) | Stmt::ClassDef(_)) {
            walk_stmt(self, statement);
        }
    }
    fn visit_expr(&mut self, expression: &'a Expr) {
        if let Expr::Lambda(_) = expression {
            return;
        }
        if let Expr::Call(call) = expression {
            if matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "setattr") {
                self.0.push(call);
            }
        }
        walk_expr(self, expression);
    }
}

impl Checker<'_> {
    pub(super) fn setattr_definitions(
        &self,
        class: Rc<Class>,
        origin: Context,
    ) -> Vec<AttributeDefinition> {
        if self
            .lineage(class.clone(), origin.clone())
            .iter()
            .any(|(class, _)| class.identity.0 == "builtins" && class.name == "type")
        {
            return Vec::new();
        }
        let Some(source) = self.source_class(&class) else {
            return Vec::new();
        };
        let mut definitions = Vec::new();
        for method in super::classes::declarations::methods(&source.body) {
            let Some(model) = class.methods.get(method.name.as_str()) else {
                continue;
            };
            if matches!(model.kind, MethodKind::Class | MethodKind::Static)
                || method
                    .parameters
                    .iter()
                    .any(|value| value.name().as_str() == "setattr")
                || super::callables::collect(&method.body, &class.identity.0, false)
                    .contains_key("setattr")
            {
                continue;
            }
            let Some(receiver) = method
                .parameters
                .posonlyargs
                .iter()
                .chain(&method.parameters.args)
                .next()
            else {
                continue;
            };
            let mut calls = Calls::default();
            calls.visit_body(&method.body);
            for call in calls.0 {
                let [Expr::Name(instance), Expr::StringLiteral(name), _, ..] =
                    call.arguments.args.as_ref()
                else {
                    continue;
                };
                if instance.id.as_str() != receiver.parameter.name.as_str()
                    || self
                        .stdlib_name_in(&call.func, origin.clone(), &mut BTreeSet::new())
                        .as_deref()
                        != Some("builtins.setattr")
                {
                    continue;
                }
                definitions.push(AttributeDefinition {
                    module: None,
                    name: name.value.to_str().into(),
                    method: method.name.to_string(),
                    range: call.range(),
                    setter: false,
                });
            }
        }
        definitions
    }
}
