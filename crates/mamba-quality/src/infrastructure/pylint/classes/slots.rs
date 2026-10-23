//! Slot lookup separates the last declaration from all possible layouts.
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    Expr, Stmt,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(super) struct Declarations {
    pub slots: Vec<Expr>,
    pub checks: Vec<Expr>,
    pub annotations: BTreeSet<String>,
    pub declared: BTreeSet<String>,
    names: BTreeMap<String, usize>,
    values: BTreeMap<String, Expr>,
}
impl Declarations {
    fn assign(&mut self, target: &Expr, value: Option<&Expr>) {
        let targets = match target {
            Expr::Tuple(value) => Some(&value.elts),
            Expr::List(value) => Some(&value.elts),
            _ => None,
        };
        if let Some(targets) = targets {
            let values = match value {
                Some(Expr::Tuple(value)) => Some(&value.elts),
                Some(Expr::List(value)) => Some(&value.elts),
                _ => None,
            };
            for (index, target) in targets.iter().enumerate() {
                self.assign(target, values.and_then(|values| values.get(index)));
            }
            return;
        }
        let Expr::Name(name) = target else {
            return;
        };
        *self.names.entry(name.id.to_string()).or_default() += 1;
        if let Some(value) = value {
            self.values.insert(name.id.to_string(), value.clone());
            self.annotations.remove(name.id.as_str());
        } else {
            self.annotations.insert(name.id.to_string());
        }
        if name.id.as_str() == "__slots__" {
            self.checks = value.into_iter().cloned().collect();
            self.slots.extend(value.cloned());
        }
    }
    fn expand(&self, expression: &Expr) -> Expr {
        let mut expression = expression.clone();
        let mut seen = BTreeSet::new();
        while let Expr::Name(name) = &expression {
            if !seen.insert(name.id.to_string()) {
                break;
            }
            let Some(value) = self.values.get(name.id.as_str()) else {
                break;
            };
            expression = value.clone();
        }
        expression
    }
}
impl<'a> Visitor<'a> for Declarations {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::Assign(value) => {
                for target in &value.targets {
                    self.assign(target, Some(&value.value));
                }
            }
            Stmt::AnnAssign(value) => self.assign(&value.target, value.value.as_deref()),
            Stmt::FunctionDef(value) => {
                *self.names.entry(value.name.to_string()).or_default() += 1;
            }
            Stmt::ClassDef(value) => {
                *self.names.entry(value.name.to_string()).or_default() += 1;
            }
            Stmt::If(value) => {
                let initial = self.checks.clone();
                self.visit_body(&value.body);
                let mut branches = self.checks.clone();
                for clause in &value.elif_else_clauses {
                    self.checks = initial.clone();
                    self.visit_body(&clause.body);
                    branches.extend(self.checks.clone());
                }
                self.checks = branches;
            }
            _ => walk_stmt(self, statement),
        }
    }
}
pub(super) fn collect(body: &[Stmt]) -> Declarations {
    let mut result = Declarations::default();
    result.visit_body(body);
    result
        .annotations
        .retain(|name| result.names.get(name) == Some(&1));
    result.declared = result.names.keys().cloned().collect();
    result.slots = result
        .slots
        .iter()
        .map(|value| result.expand(value))
        .collect();
    result.checks = result
        .checks
        .iter()
        .map(|value| result.expand(value))
        .collect();
    result
}
