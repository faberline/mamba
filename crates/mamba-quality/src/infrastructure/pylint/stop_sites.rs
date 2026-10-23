//! Generator sites retain their frame and nearest lexical exception wrapper.
use super::callables::Signature;
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    ExceptHandler, Expr, Stmt,
};
use ruff_text_size::Ranged;
use std::collections::BTreeMap;

#[derive(Clone, Default)]
pub(super) struct Site {
    pub catches: Vec<Expr>,
    pub suppressors: Vec<Expr>,
}
#[derive(Default)]
pub(super) struct Sites(pub BTreeMap<u32, Site>);
#[derive(Default)]
struct Collector {
    data: Sites,
    generator: bool,
    catches: Vec<Expr>,
    suppressors: Vec<Expr>,
}
impl Sites {
    pub(super) fn new(body: &[Stmt]) -> Self {
        let mut collector = Collector::default();
        collector.visit_body(body);
        collector.data
    }
}
impl Collector {
    fn site(&mut self, offset: u32) {
        if self.generator {
            self.data.0.insert(
                offset,
                Site {
                    catches: self.catches.clone(),
                    suppressors: self.suppressors.clone(),
                },
            );
        }
    }
}
impl<'a> Visitor<'a> for Collector {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        let previous = self.generator;
        match statement {
            Stmt::FunctionDef(function) => {
                self.generator = Signature::from_function(function).generates;
            }
            Stmt::ClassDef(_) => self.generator = false,
            Stmt::Raise(value) => self.site(value.start().to_u32()),
            Stmt::Try(attempt) => {
                let saved = self.catches.clone();
                self.catches = attempt
                    .handlers
                    .iter()
                    .filter_map(|handler| {
                        let ExceptHandler::ExceptHandler(handler) = handler;
                        handler.type_.as_deref().cloned()
                    })
                    .collect();
                self.visit_body(&attempt.body);
                for handler in &attempt.handlers {
                    self.catches.clear();
                    self.visit_except_handler(handler);
                }
                self.catches = attempt
                    .handlers
                    .iter()
                    .filter_map(|handler| {
                        let ExceptHandler::ExceptHandler(handler) = handler;
                        handler.type_.as_deref().cloned()
                    })
                    .collect();
                self.visit_body(&attempt.orelse);
                self.visit_body(&attempt.finalbody);
                self.catches = saved;
                return;
            }
            Stmt::With(value) => {
                let saved = self.suppressors.len();
                self.suppressors
                    .extend(value.items.iter().map(|item| item.context_expr.clone()));
                walk_stmt(self, statement);
                self.suppressors.truncate(saved);
                return;
            }
            _ => {}
        }
        walk_stmt(self, statement);
        self.generator = previous;
    }
    fn visit_expr(&mut self, expression: &'a Expr) {
        let previous = self.generator;
        if matches!(expression, Expr::Lambda(_)) {
            self.generator = false;
        }
        if let Expr::Call(call) = expression {
            self.site(call.start().to_u32());
        }
        walk_expr(self, expression);
        self.generator = previous;
    }
}
