//! Unpacking bindings retain conditional uncertainty and augmented expressions.
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    Expr, ExprBinOp, Stmt,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(super) struct Bindings {
    pub expressions: BTreeMap<String, Expr>,
    pub uncertain: BTreeSet<String>,
    conditional: bool,
}
impl Bindings {
    fn target(&mut self, target: &Expr, value: &Expr) {
        match target {
            Expr::Name(name) => {
                if self.conditional {
                    self.uncertain.insert(name.id.to_string());
                } else {
                    self.uncertain.remove(name.id.as_str());
                }
                self.expressions.insert(name.id.to_string(), value.clone());
            }
            Expr::Tuple(targets) => self.unpacked(&targets.elts, value),
            Expr::List(targets) => self.unpacked(&targets.elts, value),
            _ => {}
        }
    }
    fn unpacked(&mut self, targets: &[Expr], value: &Expr) {
        let values = match value {
            Expr::Tuple(value) => &value.elts,
            Expr::List(value) => &value.elts,
            _ => return,
        };
        for (target, value) in targets.iter().zip(values) {
            self.target(target, value);
        }
    }
    pub(super) fn expression(&self, name: &str) -> Option<Expr> {
        (!self.uncertain.contains(name))
            .then(|| self.expressions.get(name).cloned())
            .flatten()
    }
}
impl<'a> Visitor<'a> for Bindings {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::Assign(value) => {
                for target in &value.targets {
                    self.target(target, &value.value);
                }
            }
            Stmt::AnnAssign(value) => {
                if let Some(expression) = &value.value {
                    self.target(&value.target, expression);
                }
            }
            Stmt::AugAssign(value) => {
                if let Expr::Name(name) = value.target.as_ref() {
                    if let Some(previous) = self.expression(name.id.as_str()) {
                        self.target(
                            &value.target,
                            &Expr::BinOp(ExprBinOp {
                                node_index: Default::default(),
                                range: value.range,
                                left: Box::new(previous),
                                op: value.op,
                                right: value.value.clone(),
                            }),
                        );
                    }
                }
            }
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {}
            _ => {
                let previous = self.conditional;
                self.conditional = true;
                walk_stmt(self, statement);
                self.conditional = previous;
            }
        }
    }
}
pub(super) fn collect(body: &[Stmt]) -> Bindings {
    let mut bindings = Bindings::default();
    bindings.visit_body(body);
    bindings
}
