//! Assignment history supplies the values visible at a source read.
use super::{
    callables::Target,
    resolution::{Context, Resolved},
    values::Value,
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    Expr, Stmt,
};
use ruff_text_size::{Ranged, TextRange, TextSize};
use std::collections::BTreeMap;
use std::rc::Rc;

impl Checker<'_> {
    pub(super) fn history_target(&self, value: &Expr, context: Context) -> Option<Resolved> {
        let literal = Value::from_expression(value);
        if matches!(literal, Value::StringMethod(_, _)) {
            Some(Resolved {
                target: Target::Value(Rc::new(literal)),
                context,
            })
        } else {
            self.expression_target_in(value, context)
        }
    }
}

struct Binding {
    range: TextRange,
    value: Expr,
    conditional: bool,
}
#[derive(Default)]
pub(super) struct History {
    values: BTreeMap<String, Vec<Binding>>,
    conditional: bool,
}
impl History {
    fn target(&mut self, target: &Expr, value: &Expr, range: TextRange) {
        if let Expr::Name(name) = target {
            self.values
                .entry(name.id.to_string())
                .or_default()
                .push(Binding {
                    range,
                    value: value.clone(),
                    conditional: self.conditional,
                });
        }
    }
    pub(super) fn collect(body: &[Stmt]) -> Self {
        let mut result = Self::default();
        result.visit_body(body);
        result
    }
    pub(super) fn before(&self, name: &str, position: TextSize) -> Option<Vec<Expr>> {
        let bindings = self.values.get(name)?;
        let mut result = Vec::new();
        for binding in bindings
            .iter()
            .filter(|binding| binding.range.end() <= position)
        {
            if !binding.conditional {
                result.clear();
            }
            result.push(binding.value.clone());
        }
        Some(result)
    }
}
impl<'a> Visitor<'a> for History {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::Assign(value) => {
                for target in &value.targets {
                    self.target(target, &value.value, statement.range());
                }
            }
            Stmt::AnnAssign(value) => {
                if let Some(expression) = &value.value {
                    self.target(&value.target, expression, statement.range());
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
