//! Parent-call and enclosing-loop data are collected without changing rule traversal.
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    Expr, Stmt,
};
use ruff_text_size::Ranged;
use std::collections::{BTreeMap, BTreeSet};

pub(super) type Subscripts = BTreeMap<u32, Expr>;
#[derive(Default)]
struct Assignments(BTreeSet<String>);
impl<'a> Visitor<'a> for Assignments {
    fn visit_stmt(&mut self, value: &'a Stmt) {
        match value {
            Stmt::Assign(value) => {
                for target in &value.targets {
                    if let Expr::Name(name) = target {
                        self.0.insert(name.id.to_string());
                    }
                }
            }
            Stmt::AugAssign(value) => {
                if let Expr::Name(name) = value.target.as_ref() {
                    self.0.insert(name.id.to_string());
                }
            }
            _ => {}
        }
        walk_stmt(self, value);
    }
}
#[derive(Default)]
struct Parents {
    values: Subscripts,
    loops: Vec<BTreeSet<String>>,
}
impl<'a> Visitor<'a> for Parents {
    fn visit_stmt(&mut self, value: &'a Stmt) {
        if matches!(value, Stmt::FunctionDef(_) | Stmt::ClassDef(_)) {
            let saved = std::mem::take(&mut self.loops);
            walk_stmt(self, value);
            self.loops = saved;
            return;
        }
        if matches!(value, Stmt::For(_) | Stmt::While(_)) {
            let mut assignments = Assignments::default();
            assignments.visit_stmt(value);
            self.loops.push(assignments.0);
            walk_stmt(self, value);
            self.loops.pop();
            return;
        }
        walk_stmt(self, value);
    }
    fn visit_expr(&mut self, value: &'a Expr) {
        if let Expr::Subscript(subscript) = value {
            if let Expr::Call(call) = subscript.value.as_ref() {
                let mutated = matches!(subscript.slice.as_ref(), Expr::Name(name) if self.loops.iter().any(|values| values.contains(name.id.as_str())));
                if !mutated {
                    self.values
                        .insert(call.start().to_u32(), subscript.slice.as_ref().clone());
                }
            }
        }
        walk_expr(self, value);
    }
}
pub(super) fn subscripts(body: &[Stmt]) -> Subscripts {
    let mut parents = Parents::default();
    parents.visit_body(body);
    parents.values
}
