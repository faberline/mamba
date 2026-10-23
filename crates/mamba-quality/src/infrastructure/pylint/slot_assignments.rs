//! Attribute stores retain their assigned value and unpacking context.
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    Expr, Stmt,
};
use ruff_text_size::Ranged;
use std::collections::BTreeMap;

#[derive(Clone)]
pub(super) struct Assignment {
    pub value: Expr,
    pub direct: bool,
    pub starred: bool,
}
pub(super) type Assignments = BTreeMap<u32, Assignment>;

#[derive(Default)]
struct Collector(Assignments);
impl Collector {
    fn target(&mut self, target: &Expr, value: &Expr, direct: bool) {
        match target {
            Expr::Attribute(attribute) => {
                self.0.insert(
                    attribute.start().to_u32(),
                    Assignment {
                        value: value.clone(),
                        direct,
                        starred: false,
                    },
                );
            }
            Expr::Tuple(tuple) => self.unpacked(&tuple.elts, value),
            Expr::List(list) => self.unpacked(&list.elts, value),
            Expr::Starred(target) => {
                self.target(&target.value, value, true);
                if let Some(assignment) = self.0.get_mut(&target.value.start().to_u32()) {
                    assignment.starred = true;
                }
            }
            _ => {}
        }
    }
    fn unpacked(&mut self, targets: &[Expr], value: &Expr) {
        let values = match value {
            Expr::Tuple(tuple) => &tuple.elts,
            Expr::List(list) => &list.elts,
            _ => return,
        };
        let class_index = targets.iter().rposition(
            |target| matches!(target, Expr::Attribute(value) if value.attr.as_str() == "__class__"),
        );
        for (index, target) in targets.iter().enumerate() {
            if matches!(target, Expr::Starred(_)) {
                self.target(target, value, true);
                continue;
            }
            if !matches!(target, Expr::Attribute(_)) {
                continue;
            }
            let index = if matches!(target, Expr::Attribute(value) if value.attr.as_str() == "__class__")
            {
                class_index.unwrap_or(index)
            } else {
                index
            };
            if let Some(value) = values.get(index) {
                self.target(target, value, false);
            }
        }
    }
}
impl<'a> Visitor<'a> for Collector {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::Assign(value) => {
                for target in &value.targets {
                    self.target(target, &value.value, true);
                }
            }
            Stmt::AnnAssign(value) => {
                if let Some(expression) = &value.value {
                    self.target(&value.target, expression, true);
                }
            }
            Stmt::AugAssign(value) => self.target(&value.target, &value.value, true),
            _ => {}
        }
        walk_stmt(self, statement);
    }
}
pub(super) fn collect(body: &[Stmt]) -> Assignments {
    let mut collector = Collector::default();
    collector.visit_body(body);
    collector.0
}
