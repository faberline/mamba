//! Recursive index scans retain assignment-target and immediate-delete boundaries.
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    Comprehension, Expr, ExprSubscript, Stmt,
};
use ruff_text_size::{Ranged, TextRange};
use std::collections::BTreeSet;

pub(super) struct Subscript<'a> {
    pub value: &'a ExprSubscript,
    pub assigned: bool,
    pub deleted: bool,
}
#[derive(Default)]
pub(super) struct Scan<'a> {
    pub values: Vec<Subscript<'a>>,
    pub nested_loops: bool,
    pub conditional: bool,
    assignments: BTreeSet<(u32, u32)>,
    deletions: BTreeSet<(u32, u32)>,
}
fn key(range: TextRange) -> (u32, u32) {
    (range.start().to_u32(), range.end().to_u32())
}
impl Scan<'_> {
    fn target(&mut self, target: &Expr) {
        match target {
            Expr::Subscript(value) => {
                self.assignments.insert(key(value.range()));
            }
            Expr::Tuple(value) => {
                for target in &value.elts {
                    self.target(target);
                }
            }
            Expr::List(value) => {
                for target in &value.elts {
                    self.target(target);
                }
            }
            _ => {}
        }
    }
}
impl<'a> Visitor<'a> for Scan<'a> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::Assign(value) => {
                for target in &value.targets {
                    self.target(target);
                }
            }
            Stmt::AugAssign(value) => self.target(&value.target),
            Stmt::Delete(value) => {
                for target in &value.targets {
                    if let Expr::Subscript(value) = target {
                        self.deletions.insert(key(value.range()));
                    }
                }
            }
            Stmt::For(value) if !value.is_async => self.nested_loops = true,
            Stmt::While(_) => self.nested_loops = true,
            Stmt::If(_) => self.conditional = true,
            _ => {}
        }
        walk_stmt(self, statement);
    }
    fn visit_expr(&mut self, expression: &'a Expr) {
        if let Expr::Subscript(value) = expression {
            self.values.push(Subscript {
                value,
                assigned: self.assignments.contains(&key(value.range())),
                deleted: self.deletions.contains(&key(value.range())),
            });
        }
        walk_expr(self, expression);
    }
}
pub(super) fn body(body: &[Stmt]) -> Scan<'_> {
    let mut scan = Scan::default();
    scan.visit_body(body);
    scan
}
pub(super) fn comprehension<'a>(generators: &'a [Comprehension], values: &[&'a Expr]) -> Scan<'a> {
    let mut scan = Scan::default();
    for value in values {
        scan.visit_expr(value);
    }
    for generator in generators {
        scan.visit_expr(&generator.target);
        scan.visit_expr(&generator.iter);
        for condition in &generator.ifs {
            scan.visit_expr(condition);
        }
    }
    scan
}
