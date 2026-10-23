//! Function metrics retain original statement accumulation across nested scopes.
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_except_handler, walk_stmt, Visitor},
    ExceptHandler, Expr, Stmt,
};
use ruff_text_size::Ranged;
use std::collections::BTreeMap;

#[derive(Default, Clone, Copy)]
pub(super) struct Counts {
    pub returns: usize,
    pub branches: usize,
    pub statements: usize,
}
#[derive(Default)]
pub(super) struct Metrics {
    pub functions: BTreeMap<u32, Counts>,
    active: Vec<u32>,
    scopes: Vec<Option<u32>>,
}
impl Metrics {
    pub(super) fn new(body: &[Stmt]) -> Self {
        let mut result = Self::default();
        result.visit_body(body);
        result
    }
    fn statements(&mut self, count: usize) {
        for key in &self.active {
            self.functions.get_mut(key).unwrap().statements += count;
        }
    }
    fn branches(&mut self, count: usize) {
        if let Some(Some(key)) = self.scopes.last() {
            self.functions.get_mut(key).unwrap().branches += count;
        }
    }
}
impl<'a> Visitor<'a> for Metrics {
    fn visit_except_handler(&mut self, value: &'a ExceptHandler) {
        self.statements(1);
        walk_except_handler(self, value);
    }
    fn visit_body(&mut self, body: &'a [Stmt]) {
        for statement in body {
            self.visit_stmt(statement);
        }
    }
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::FunctionDef(function) => {
                let key = function.start().to_u32();
                self.functions.insert(
                    key,
                    Counts {
                        statements: 1,
                        ..Default::default()
                    },
                );
                self.active.push(key);
                self.scopes.push(Some(key));
                self.visit_body(&function.body[usize::from(super::docstring(&function.body))..]);
                self.scopes.pop();
                self.active.pop();
                return;
            }
            Stmt::ClassDef(class) => {
                self.scopes.push(None);
                self.visit_body(&class.body[usize::from(super::docstring(&class.body))..]);
                self.scopes.pop();
                return;
            }
            Stmt::Return(_) => {
                if let Some(key) = self.active.last() {
                    self.functions.get_mut(key).unwrap().returns += 1;
                }
            }
            Stmt::If(value) => {
                let branches = 1 + value
                    .elif_else_clauses
                    .iter()
                    .filter(|clause| {
                        clause.test.is_some()
                            || !(clause.body.len() == 1
                                && matches!(clause.body.first(), Some(Stmt::If(_))))
                    })
                    .count();
                self.branches(branches);
                self.statements(branches);
            }
            Stmt::Try(value) => {
                let branches = value.handlers.len()
                    + usize::from(!value.orelse.is_empty())
                    + usize::from(!value.finalbody.is_empty());
                self.branches(branches);
                self.statements(branches);
            }
            Stmt::While(value) => self.branches(1 + usize::from(!value.orelse.is_empty())),
            Stmt::For(value) => self.branches(1 + usize::from(!value.orelse.is_empty())),
            Stmt::Match(value) => {
                self.statements(1);
                self.branches(value.cases.len());
            }
            _ => self.statements(1),
        }
        walk_stmt(self, statement);
    }
}
pub(super) fn boolean_count(value: &Expr) -> usize {
    if let Expr::BoolOp(value) = value {
        value.values.iter().map(boolean_count).sum()
    } else {
        1
    }
}
