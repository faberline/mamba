//! Field sites retain the nearest call or class and direct annotation ownership.
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    Expr, Stmt,
};
use ruff_text_size::{Ranged, TextRange};
use std::collections::BTreeMap;

#[derive(Clone, Default)]
pub(super) enum Parent {
    #[default]
    Root,
    Class(Vec<Expr>),
    Call(Expr),
}
#[derive(Clone)]
pub(super) struct Site {
    pub parent: Parent,
    pub direct_annotation: bool,
}
#[derive(Default)]
pub(super) struct Sites(pub BTreeMap<u32, Site>);
#[derive(Default)]
struct Collector {
    sites: Sites,
    parent: Parent,
    annotation: Option<TextRange>,
}
pub(super) fn raw_name(expression: &Expr) -> Option<&str> {
    match expression {
        Expr::Name(value) => Some(value.id.as_str()),
        Expr::Attribute(value) => Some(value.attr.as_str()),
        _ => None,
    }
}
impl<'a> Visitor<'a> for Collector {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        let annotation = self.annotation;
        self.annotation = match statement {
            Stmt::AnnAssign(value) => value.value.as_deref().map(Ranged::range),
            _ => None,
        };
        if let Stmt::ClassDef(value) = statement {
            let parent = std::mem::replace(
                &mut self.parent,
                Parent::Class(
                    value
                        .decorator_list
                        .iter()
                        .map(|value| value.expression.clone())
                        .collect(),
                ),
            );
            walk_stmt(self, statement);
            self.parent = parent;
        } else {
            walk_stmt(self, statement);
        }
        self.annotation = annotation;
    }
    fn visit_expr(&mut self, expression: &'a Expr) {
        if let Expr::Call(call) = expression {
            if raw_name(&call.func) == Some("field") {
                self.sites.0.insert(
                    call.start().to_u32(),
                    Site {
                        parent: self.parent.clone(),
                        direct_annotation: self.annotation == Some(call.range()),
                    },
                );
            }
            let parent = std::mem::replace(&mut self.parent, Parent::Call((*call.func).clone()));
            walk_expr(self, expression);
            self.parent = parent;
        } else {
            walk_expr(self, expression);
        }
    }
}
impl Sites {
    pub(super) fn new(body: &[Stmt]) -> Self {
        let mut collector = Collector::default();
        collector.visit_body(body);
        collector.sites
    }
}
