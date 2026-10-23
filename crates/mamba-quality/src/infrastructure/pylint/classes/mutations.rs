//! Class and constructor attribute stores retain their inferred owner category.
use super::{callables::Target, values::Value, AttributeDefinition, MethodKind};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    Expr, Stmt, StmtFunctionDef,
};
use ruff_text_size::Ranged;
use std::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

pub(super) struct Mutations {
    class: String,
    class_receivers: BTreeSet<String>,
    instance_receivers: BTreeSet<String>,
    method: String,
    pub members: BTreeMap<String, Target>,
    pub attributes: BTreeMap<String, Target>,
    pub definitions: Vec<AttributeDefinition>,
}
impl Mutations {
    pub(super) fn new(class: &str, function: &StmtFunctionDef, kind: MethodKind) -> Self {
        let first = function
            .parameters
            .iter()
            .next()
            .map(|value| value.name().to_string());
        Self {
            class: class.into(),
            class_receivers: if kind == MethodKind::Class {
                first.into_iter().collect()
            } else {
                BTreeSet::new()
            },
            instance_receivers: BTreeSet::new(),
            method: function.name.to_string(),
            members: BTreeMap::new(),
            attributes: BTreeMap::new(),
            definitions: Vec::new(),
        }
    }
    fn assign(&mut self, target: &Expr, value: Option<&Expr>) {
        if let Expr::Name(name) = target {
            let creates_instance = value.is_some_and(|value| {
                let Expr::Call(call) = value else { return false; };
                let Expr::Attribute(attribute) = call.func.as_ref() else { return false; };
                attribute.attr.as_str() == "__new__"
                    && matches!(attribute.value.as_ref(), Expr::Name(value) if value.id.as_str() == "object")
                    && matches!(call.arguments.args.first(), Some(Expr::Name(value)) if self.class_receivers.contains(value.id.as_str()) || value.id.as_str() == self.class)
            });
            if creates_instance {
                self.instance_receivers.insert(name.id.to_string());
            }
            return;
        }
        match target {
            Expr::Tuple(value) => {
                for target in &value.elts {
                    self.assign(target, None);
                }
            }
            Expr::List(value) => {
                for target in &value.elts {
                    self.assign(target, None);
                }
            }
            Expr::Attribute(attribute) => {
                let Expr::Name(receiver) = attribute.value.as_ref() else {
                    return;
                };
                let target = Target::Value(Rc::new(
                    value.map(Value::from_expression).unwrap_or(Value::Unknown),
                ));
                if receiver.id.as_str() == self.class
                    || self.class_receivers.contains(receiver.id.as_str())
                {
                    self.members.insert(attribute.attr.to_string(), target);
                } else if self.instance_receivers.contains(receiver.id.as_str()) {
                    self.attributes.insert(attribute.attr.to_string(), target);
                    self.definitions.push(AttributeDefinition {
                        module: None,
                        name: attribute.attr.to_string(),
                        method: self.method.clone(),
                        range: attribute.range(),
                        setter: false,
                    });
                }
            }
            _ => {}
        }
    }
}
impl<'a> Visitor<'a> for Mutations {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::Assign(value) => {
                for target in &value.targets {
                    self.assign(target, Some(&value.value));
                }
            }
            Stmt::AnnAssign(value) => self.assign(&value.target, value.value.as_deref()),
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {}
            Stmt::AugAssign(_) => {}
            _ => walk_stmt(self, statement),
        }
    }
    fn visit_expr(&mut self, expression: &'a Expr) {
        if let Expr::Attribute(value) = expression {
            if value.ctx.is_store() {
                self.assign(expression, None);
            }
        }
        walk_expr(self, expression);
    }
}
