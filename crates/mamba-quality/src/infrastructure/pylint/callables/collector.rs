//! Binding collection keeps first declarations without changing last-value lookup.
use super::super::{classes, values};
use super::{Collector, Signature, Target};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    Expr, Stmt,
};
use std::rc::Rc;

impl Collector {
    fn define(&mut self, name: String, target: Target) {
        if self.record_first {
            self.first_targets
                .entry(name.clone())
                .or_insert_with(|| target.clone());
        }
        self.targets.insert(name, target);
    }
    fn assign(&mut self, target: &Expr, expression: &Expr) {
        let targets = match target {
            Expr::Tuple(value) => Some(&value.elts),
            Expr::List(value) => Some(&value.elts),
            _ => None,
        };
        if let Some(targets) = targets {
            let values = match expression {
                Expr::Tuple(value) => Some(&value.elts),
                Expr::List(value) => Some(&value.elts),
                _ => None,
            };
            for (index, target) in targets.iter().enumerate() {
                if let Some(value) = values.and_then(|values| values.get(index)) {
                    self.assign(target, value);
                } else {
                    self.unknown_target(target);
                }
            }
            return;
        }
        if let Expr::Name(name) = target {
            let value = match expression {
                Expr::Lambda(lambda) => Target::Function(Rc::new(Signature::from_lambda(lambda))),
                Expr::Name(_) | Expr::Attribute(_) => values::path(expression)
                    .map(Target::Reference)
                    .unwrap_or_else(|| {
                        Target::Value(Rc::new(values::Value::from_expression(expression)))
                    }),
                Expr::Call(call) => Target::Construct(Rc::new(call.clone())),
                _ => Target::Value(Rc::new(values::Value::from_expression(expression))),
            };
            self.define(name.id.to_string(), value);
        }
    }
    fn unknown_target(&mut self, target: &Expr) {
        match target {
            Expr::Name(name) => self.define(
                name.id.to_string(),
                Target::Value(Rc::new(values::Value::Unknown)),
            ),
            Expr::Tuple(value) => value
                .elts
                .iter()
                .for_each(|value| self.unknown_target(value)),
            Expr::List(value) => value
                .elts
                .iter()
                .for_each(|value| self.unknown_target(value)),
            Expr::Starred(value) => self.unknown_target(&value.value),
            _ => {}
        }
    }
}
impl<'a> Visitor<'a> for Collector {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::FunctionDef(function) => {
                self.define(
                    function.name.to_string(),
                    Target::Function(Rc::new(Signature::from_function(function))),
                );
            }
            Stmt::ClassDef(class) => {
                self.define(
                    class.name.to_string(),
                    Target::Class(Rc::new(classes::Class::from_statement(
                        class,
                        &self.current,
                        self.is_package,
                    ))),
                );
            }
            Stmt::Assign(assignment) => {
                for target in &assignment.targets {
                    self.assign(target, &assignment.value);
                }
            }
            Stmt::AnnAssign(assignment) => {
                if let Some(value) = &assignment.value {
                    self.assign(&assignment.target, value);
                } else if self.record_first {
                    if let Expr::Name(name) = assignment.target.as_ref() {
                        self.define(
                            name.id.to_string(),
                            Target::Value(Rc::new(values::Value::Unknown)),
                        );
                    }
                }
            }
            Stmt::For(value) => {
                let first = match value.iter.as_ref() {
                    Expr::List(value) => value.elts.first(),
                    Expr::Tuple(value) => value.elts.first(),
                    _ => None,
                };
                if let Some(first) = first {
                    self.assign(&value.target, first);
                } else {
                    self.unknown_target(&value.target);
                }
                self.visit_body(&value.body);
                self.visit_body(&value.orelse);
            }
            Stmt::Import(_) | Stmt::ImportFrom(_) => {
                for import in crate::infrastructure::project::imports(
                    std::slice::from_ref(statement),
                    &self.current,
                    self.is_package,
                ) {
                    self.define(import.local.clone(), Target::Imported(import));
                }
            }
            _ => walk_stmt(self, statement),
        }
    }
    fn visit_expr(&mut self, expression: &'a Expr) {
        if let Expr::Named(value) = expression {
            self.assign(&value.target, &value.value);
        }
        if !matches!(expression, Expr::Lambda(_)) {
            walk_expr(self, expression);
        }
    }
}
