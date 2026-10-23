//! Uses after a unique loop target retain the original uncertainty warning.
use super::{callables::Target, resolution::Context, Checker};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    Expr, ExprName, Stmt,
};
use ruff_text_size::Ranged;
use std::collections::BTreeMap;

#[derive(Clone)]
struct Loop {
    end: u32,
    iter: Expr,
    terminated: bool,
}
#[derive(Default)]
struct Frame {
    counts: BTreeMap<String, usize>,
    loops: BTreeMap<String, Loop>,
    uses: Vec<(String, u32)>,
}
#[derive(Default)]
pub(super) struct Loops {
    reports: BTreeMap<u32, Loop>,
    frames: Vec<Frame>,
}
impl Loops {
    pub(super) fn new(body: &[Stmt]) -> Self {
        let mut value = Self::default();
        value.frames.push(Frame::default());
        value.visit_body(body);
        value.finish();
        value
    }
    fn finish(&mut self) {
        let frame = self.frames.pop().unwrap();
        for (name, offset) in frame.uses {
            if frame.counts.get(&name) != Some(&1) {
                continue;
            }
            if let Some(value) = frame.loops.get(&name) {
                if offset > value.end && !value.terminated {
                    self.reports.insert(offset, value.clone());
                }
            }
        }
    }
}
impl<'a> Visitor<'a> for Loops {
    fn visit_stmt(&mut self, value: &'a Stmt) {
        match value {
            Stmt::FunctionDef(value) => {
                let frame = self.frames.last_mut().unwrap();
                *frame.counts.entry(value.name.to_string()).or_default() += 1;
                let mut frame = Frame::default();
                for parameter in value.parameters.iter() {
                    *frame
                        .counts
                        .entry(parameter.name().to_string())
                        .or_default() += 1;
                }
                self.frames.push(frame);
                self.visit_body(&value.body);
                self.finish();
                return;
            }
            Stmt::ClassDef(value) => {
                let frame = self.frames.last_mut().unwrap();
                *frame.counts.entry(value.name.to_string()).or_default() += 1;
                self.frames.push(Frame::default());
                self.visit_body(&value.body);
                self.finish();
                return;
            }
            Stmt::For(value) => {
                let terminated = value.orelse.iter().any(|value| {
                    matches!(
                        value,
                        Stmt::Return(_) | Stmt::Raise(_) | Stmt::Break(_) | Stmt::Continue(_)
                    )
                });
                let frame = self.frames.last_mut().unwrap();
                for name in super::local_rules::target_names(&value.target) {
                    frame.loops.insert(
                        name,
                        Loop {
                            end: value.end().to_u32(),
                            iter: (*value.iter).clone(),
                            terminated,
                        },
                    );
                }
            }
            _ => {}
        }
        walk_stmt(self, value);
    }
    fn visit_expr(&mut self, value: &'a Expr) {
        match value {
            Expr::Name(value) => {
                let frame = self.frames.last_mut().unwrap();
                if value.ctx.is_store() {
                    *frame.counts.entry(value.id.to_string()).or_default() += 1;
                } else if value.ctx.is_load() {
                    frame
                        .uses
                        .push((value.id.to_string(), value.start().to_u32()));
                }
            }
            Expr::Lambda(_)
            | Expr::ListComp(_)
            | Expr::SetComp(_)
            | Expr::DictComp(_)
            | Expr::Generator(_) => return,
            _ => {}
        }
        walk_expr(self, value);
    }
}
impl Checker<'_> {
    pub(super) fn check_loop_variable(&mut self, name: &ExprName) {
        let Some(value) = self.loop_variables.reports.get(&name.start().to_u32()) else {
            return;
        };
        let origin = Context::Scope(self.scopes.current_index());
        let safe=self.expression_target_in(&value.iter,origin.clone()).is_some_and(|resolved|matches!(resolved.target,Target::Instance(class,_) if class.identity.0=="builtins" && class.name=="range")) || self.unpacked_value(&value.iter,origin).is_some_and(|value|matches!(value.kind,"list" | "tuple" | "dict" | "set" | "frozenset") && value.elements.is_some_and(|values|!values.is_empty()));
        if !safe {
            self.emit(
                "W0631",
                "undefined-loop-variable",
                &format!("Using possibly undefined loop variable '{}'", name.id),
                name.range(),
            );
        }
    }
}
