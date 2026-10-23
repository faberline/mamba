//! The first receiver assignment in an explicit initializer initializes a Final.
use super::*;
use quality_syntax::python_ast::visitor::{walk_stmt, Visitor};
use std::collections::BTreeMap;

pub(super) fn collect(class: &StmtClassDef) -> BTreeMap<String, ruff_text_size::TextSize> {
    let mut result = BTreeMap::new();
    for statement in &class.body {
        let Stmt::FunctionDef(function) = statement else {
            continue;
        };
        if function.name.as_str() != "__init__" {
            continue;
        }
        let Some(receiver) = function.parameters.iter().next() else {
            continue;
        };
        let mut visitor = Initializers {
            receiver: receiver.name().as_str(),
            result: &mut result,
        };
        for statement in &function.body {
            visitor.visit_stmt(statement);
        }
    }
    result
}

struct Initializers<'a> {
    receiver: &'a str,
    result: &'a mut BTreeMap<String, ruff_text_size::TextSize>,
}

impl Initializers<'_> {
    fn target(&mut self, expression: &Expr) {
        let Expr::Attribute(attribute) = expression else {
            return;
        };
        if matches!(attribute.value.as_ref(), Expr::Name(name) if name.id.as_str() == self.receiver)
        {
            self.result
                .entry(attribute.attr.to_string())
                .or_insert(expression.start());
        }
    }
}

impl<'a> Visitor<'a> for Initializers<'_> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::Assign(assignment) => {
                for target in &assignment.targets {
                    self.target(target);
                }
            }
            Stmt::AnnAssign(assignment) if assignment.value.is_some() => {
                self.target(&assignment.target)
            }
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {}
            _ => walk_stmt(self, statement),
        }
    }
}
