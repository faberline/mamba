//! Closure uses retain visible lexical bindings and containing-loop exemptions.
use super::super::project::symbols::Symbols;
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    Comprehension, Expr, Parameters, Stmt,
};
use ruff_text_size::Ranged;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy)]
enum Origin {
    Loop(u32),
    Comprehension,
}
#[derive(Default)]
struct Origins {
    values: BTreeMap<String, Vec<(u32, Option<Origin>)>>,
    loops: Vec<u32>,
}
impl<'a> Visitor<'a> for Origins {
    fn visit_stmt(&mut self, value: &'a Stmt) {
        let name = match value {
            Stmt::FunctionDef(value) => Some(value.name.as_str()),
            Stmt::ClassDef(value) => Some(value.name.as_str()),
            _ => None,
        };
        if let Some(name) = name {
            self.values.entry(name.into()).or_default().push((
                value.start().to_u32(),
                self.loops.last().map(|value| Origin::Loop(*value)),
            ));
            return;
        }
        if let Stmt::For(loop_) = value {
            self.loops.push(loop_.start().to_u32());
            walk_stmt(self, value);
            self.loops.pop();
            return;
        }
        walk_stmt(self, value);
    }
    fn visit_expr(&mut self, value: &'a Expr) {
        if matches!(
            value,
            Expr::Lambda(_)
                | Expr::ListComp(_)
                | Expr::SetComp(_)
                | Expr::DictComp(_)
                | Expr::Generator(_)
        ) {
            return;
        }
        if let Expr::Name(name) = value {
            if name.ctx.is_store() {
                self.values.entry(name.id.to_string()).or_default().push((
                    name.start().to_u32(),
                    self.loops.last().map(|value| Origin::Loop(*value)),
                ));
            }
        }
        walk_expr(self, value);
    }
}
struct Frame {
    bindings: BTreeMap<String, Vec<(u32, Option<Origin>)>>,
    closure: bool,
    exempt: bool,
}
impl Frame {
    fn body(body: &[Stmt], parameters: Option<&Parameters>, closure: bool) -> Self {
        let mut symbols = Symbols::collect(body, "", false);
        if let Some(parameters) = parameters {
            symbols.parameters(parameters);
        }
        let mut origins = Origins::default();
        origins.visit_body(body);
        Self {
            bindings: symbols
                .bindings
                .into_iter()
                .map(|(name, value)| {
                    let values = origins
                        .values
                        .remove(&name)
                        .unwrap_or_else(|| vec![(value.range.start().to_u32(), None)]);
                    (name, values)
                })
                .collect(),
            closure,
            exempt: false,
        }
    }
    fn comprehension(generators: &[Comprehension]) -> Self {
        Self {
            bindings: generators
                .iter()
                .flat_map(|value| super::local_rules::target_names(&value.target))
                .map(|name| (name, vec![(0, Some(Origin::Comprehension))]))
                .collect(),
            closure: false,
            exempt: false,
        }
    }
}
#[derive(Default)]
struct Uses {
    frames: Vec<Frame>,
    loops: Vec<u32>,
    warnings: BTreeSet<u32>,
    called: bool,
    returned: bool,
}
impl Uses {
    fn comprehension(&mut self, generators: &[Comprehension], values: &[&Expr]) {
        if let Some(first) = generators.first() {
            self.visit_expr(&first.iter);
        }
        self.frames.push(Frame::comprehension(generators));
        for (index, generator) in generators.iter().enumerate() {
            if index != 0 {
                self.visit_expr(&generator.iter);
            }
            for condition in &generator.ifs {
                self.visit_expr(condition);
            }
        }
        for value in values {
            self.visit_expr(value);
        }
        self.frames.pop();
    }
}
impl<'a> Visitor<'a> for Uses {
    fn visit_stmt(&mut self, value: &'a Stmt) {
        let returned = self.returned;
        self.returned = matches!(value, Stmt::Return(_));
        match value {
            Stmt::FunctionDef(function) => {
                self.visit_parameters(&function.parameters);
                for decorator in &function.decorator_list {
                    self.visit_expr(&decorator.expression);
                }
                self.frames.push(Frame::body(
                    &function.body,
                    Some(&function.parameters),
                    true,
                ));
                self.visit_body(&function.body);
                self.frames.pop();
            }
            Stmt::ClassDef(class) => {
                self.frames.push(Frame::body(&class.body, None, false));
                self.visit_body(&class.body);
                self.frames.pop();
            }
            Stmt::For(loop_) => {
                self.loops.push(loop_.start().to_u32());
                walk_stmt(self, value);
                self.loops.pop();
            }
            _ => walk_stmt(self, value),
        }
        self.returned = returned;
    }
    fn visit_expr(&mut self, value: &'a Expr) {
        match value {
            Expr::Name(name) if name.ctx.is_load() => {
                let frame = self.frames.last().unwrap();
                if frame.closure && !frame.bindings.contains_key(name.id.as_str()) {
                    if let Some(bindings) = self
                        .frames
                        .iter()
                        .rev()
                        .skip(1)
                        .find_map(|frame| frame.bindings.get(name.id.as_str()))
                    {
                        let origin = bindings
                            .iter()
                            .filter(|(offset, _)| *offset <= name.start().to_u32())
                            .max_by_key(|(offset, _)| offset)
                            .and_then(|(_, origin)| *origin);
                        if matches!(origin, Some(Origin::Comprehension))
                            || (!frame.exempt
                                && matches!(origin, Some(Origin::Loop(value)) if self.loops.contains(&value)))
                        {
                            self.warnings.insert(name.start().to_u32());
                        }
                    }
                }
            }
            Expr::Lambda(lambda) => {
                if let Some(parameters) = &lambda.parameters {
                    self.visit_parameters(parameters);
                }
                let mut frame = Frame::body(&[], lambda.parameters.as_deref(), true);
                frame.exempt = self.called || self.returned;
                self.frames.push(frame);
                let called = std::mem::replace(&mut self.called, false);
                self.visit_expr(&lambda.body);
                self.called = called;
                self.frames.pop();
                return;
            }
            Expr::Call(call) => {
                let called = std::mem::replace(&mut self.called, true);
                self.visit_expr(&call.func);
                self.called = false;
                for argument in &call.arguments.args {
                    self.visit_expr(argument);
                }
                for keyword in &call.arguments.keywords {
                    self.visit_expr(&keyword.value);
                }
                self.called = called;
                return;
            }
            Expr::ListComp(comp) => {
                self.comprehension(&comp.generators, &[&comp.elt]);
                return;
            }
            Expr::SetComp(comp) => {
                self.comprehension(&comp.generators, &[&comp.elt]);
                return;
            }
            Expr::Generator(comp) => {
                self.comprehension(&comp.generators, &[&comp.elt]);
                return;
            }
            Expr::DictComp(comp) => {
                let mut values = vec![comp.value.as_ref()];
                if let Some(key) = &comp.key {
                    values.push(key);
                }
                self.comprehension(&comp.generators, &values);
                return;
            }
            _ => {}
        }
        let called = std::mem::replace(&mut self.called, false);
        walk_expr(self, value);
        self.called = called;
    }
}
pub(super) fn uses(body: &[Stmt]) -> BTreeSet<u32> {
    let mut uses = Uses::default();
    uses.frames.push(Frame::body(body, None, false));
    uses.visit_body(body);
    uses.warnings
}
