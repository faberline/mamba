//! Iterator uses retain lexical loop ancestors and direct repeating-loop tails.
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    Expr, Stmt,
};
use ruff_text_size::Ranged;
use std::collections::BTreeMap;

pub(super) struct Tail {
    pub breaks: bool,
    pub exits: bool,
}
pub(super) struct Loop {
    pub early: bool,
    pub iterable_name: Option<String>,
    pub children: BTreeMap<u32, Tail>,
}
#[derive(Default)]
pub(super) struct Sites {
    pub ancestors: BTreeMap<u32, Vec<u32>>,
    pub loops: BTreeMap<u32, Loop>,
}
#[derive(Default)]
struct Collector {
    data: Sites,
    loops: Vec<u32>,
}
impl<'a> Visitor<'a> for Collector {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if matches!(statement, Stmt::FunctionDef(_) | Stmt::ClassDef(_)) {
            let loops = std::mem::take(&mut self.loops);
            walk_stmt(self, statement);
            self.loops = loops;
            return;
        }
        if let Stmt::For(value) = statement {
            let children = value
                .body
                .iter()
                .enumerate()
                .map(|(index, statement)| {
                    let after = &value.body[index + 1..];
                    (
                        statement.start().to_u32(),
                        Tail {
                            breaks: after.iter().any(|value| matches!(value, Stmt::Break(_))),
                            exits: after
                                .iter()
                                .any(|value| matches!(value, Stmt::Return(_) | Stmt::Raise(_))),
                        },
                    )
                })
                .collect();
            self.data.loops.insert(
                value.start().to_u32(),
                Loop {
                    early: super::repeated_iterator_exits::early(value),
                    iterable_name: if let Expr::Name(value) = value.iter.as_ref() {
                        Some(value.id.to_string())
                    } else {
                        None
                    },
                    children,
                },
            );
            self.loops.push(value.start().to_u32());
            walk_stmt(self, statement);
            self.loops.pop();
        } else {
            walk_stmt(self, statement);
        }
    }
    fn visit_expr(&mut self, expression: &'a Expr) {
        if !self.loops.is_empty() {
            self.data.ancestors.insert(
                expression.start().to_u32(),
                self.loops.iter().rev().copied().collect(),
            );
        }
        walk_expr(self, expression);
    }
}
impl Sites {
    pub(super) fn new(body: &[Stmt]) -> Self {
        let mut collector = Collector::default();
        collector.visit_body(body);
        collector.data
    }
}
