//! Class access scans retain lexical frames and direct receiver syntax.
use super::{
    classes::{method_kind, MethodKind},
    member_access_context::{catches, Parents, Part},
};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    ExceptHandler, Expr, Stmt,
};
use ruff_text_size::{Ranged, TextRange};
use std::collections::BTreeMap;

#[derive(Clone)]
pub(super) struct Node {
    pub range: TextRange,
    pub frame: u32,
    pub parents: Parents,
}
pub(super) struct Access {
    pub node: Node,
    pub call: bool,
}
#[derive(Default)]
pub(super) struct Data {
    pub accesses: Vec<(String, Vec<Access>)>,
    pub definitions: BTreeMap<String, Vec<Node>>,
    pub frames: BTreeMap<u32, (String, bool)>,
}
#[derive(Default)]
struct Collector {
    data: Data,
    parents: Parents,
    receiver: Option<String>,
    instance: bool,
    frame: u32,
    augmented: bool,
    call: Option<TextRange>,
}
impl Collector {
    fn node(&self, range: TextRange) -> Node {
        Node {
            range,
            frame: self.frame,
            parents: self.parents.clone(),
        }
    }
    fn define(&mut self, target: &Expr) {
        match target {
            Expr::Attribute(attribute)
                if self.instance
                    && matches!(attribute.value.as_ref(), Expr::Name(value) if self.receiver.as_deref() == Some(value.id.as_str())) =>
            {
                let node = self.node(attribute.range());
                self.data
                    .definitions
                    .entry(attribute.attr.to_string())
                    .or_default()
                    .push(node);
            }
            Expr::Tuple(value) => {
                for target in &value.elts {
                    self.define(target);
                }
            }
            Expr::List(value) => {
                for target in &value.elts {
                    self.define(target);
                }
            }
            Expr::Starred(value) => self.define(&value.value),
            _ => {}
        }
    }
    fn body(&mut self, body: &[Stmt], id: u32, part: Part) {
        self.parents.push((id, part));
        self.visit_body(body);
        self.parents.pop();
    }
}
impl<'a> Visitor<'a> for Collector {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        let id = statement.start().to_u32();
        if let Stmt::Try(attempt) = statement {
            self.body(&attempt.body, id, Part::Body);
            for (index, handler) in attempt.handlers.iter().enumerate() {
                let ExceptHandler::ExceptHandler(handler) = handler;
                self.body(
                    &handler.body,
                    id,
                    Part::Handler(index, catches(handler.type_.as_deref())),
                );
            }
            self.body(&attempt.orelse, id, Part::Else);
            self.body(&attempt.finalbody, id, Part::Finally);
            return;
        }
        if let Stmt::FunctionDef(function) = statement {
            let direct = self.frame == 0;
            self.data
                .frames
                .insert(id, (function.name.to_string(), direct));
            let previous = (self.frame, self.receiver.clone(), self.instance);
            if direct {
                self.receiver = function
                    .parameters
                    .iter()
                    .next()
                    .map(|value| value.name().to_string());
                self.instance = matches!(
                    method_kind(function),
                    MethodKind::Instance | MethodKind::Property
                );
            } else if function
                .parameters
                .iter()
                .any(|value| self.receiver.as_deref() == Some(value.name().as_str()))
            {
                self.instance = false;
            }
            self.frame = id;
            self.body(&function.body, id, Part::Other);
            (self.frame, self.receiver, self.instance) = previous;
            return;
        }
        if matches!(statement, Stmt::ClassDef(_)) {
            return;
        }
        self.parents.push((id, Part::Other));
        match statement {
            Stmt::Assign(value) => {
                for target in &value.targets {
                    self.define(target);
                }
            }
            Stmt::AnnAssign(value) if value.value.is_some() => self.define(&value.target),
            Stmt::For(value) => self.define(&value.target),
            Stmt::With(value) => {
                for item in &value.items {
                    if let Some(target) = &item.optional_vars {
                        self.define(target);
                    }
                }
            }
            _ => {}
        }
        let previous = self.augmented;
        self.augmented = matches!(statement, Stmt::AugAssign(_));
        walk_stmt(self, statement);
        self.augmented = previous;
        self.parents.pop();
    }
    fn visit_expr(&mut self, expression: &'a Expr) {
        if let Expr::Lambda(lambda) = expression {
            let previous = self.frame;
            self.frame = lambda.start().to_u32();
            walk_expr(self, expression);
            self.frame = previous;
            return;
        }
        if let Expr::Attribute(attribute) = expression {
            if (attribute.ctx.is_load() || (attribute.ctx.is_store() && self.augmented))
                && matches!(attribute.value.as_ref(), Expr::Name(value) if self.receiver.as_deref() == Some(value.id.as_str()))
            {
                let access = Access {
                    node: self.node(attribute.range()),
                    call: self.call == Some(attribute.range()),
                };
                if let Some((_, accesses)) = self
                    .data
                    .accesses
                    .iter_mut()
                    .find(|(name, _)| name == attribute.attr.as_str())
                {
                    accesses.push(access);
                } else {
                    self.data
                        .accesses
                        .push((attribute.attr.to_string(), vec![access]));
                }
            }
        }
        let previous = self.call;
        if let Expr::Call(call) = expression {
            self.call = Some(call.func.range());
        }
        walk_expr(self, expression);
        self.call = previous;
    }
}
pub(super) fn collect(body: &[Stmt]) -> Data {
    let mut collector = Collector::default();
    collector.visit_body(body);
    collector.data
}
