//! Statement ancestry retains direct body membership and physical statement lines.
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    ExceptHandler, Expr, InterpolatedStringElement, Stmt,
};
use ruff_source_file::LineIndex;
use ruff_text_size::{Ranged, TextRange};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Module,
    Function,
    Class,
    Other,
    Try,
}

#[derive(Clone, Copy)]
struct Parent {
    kind: Kind,
    line: u32,
    doc: bool,
}

#[derive(Clone, Copy)]
pub(super) struct Context {
    pub parent: Kind,
    pub previous_line: u32,
    pub misplaced_future: bool,
    pub siblings: usize,
    pub doc: bool,
    pub attribute_doc: bool,
    pub next: Option<TextRange>,
}

#[derive(Default)]
pub(super) struct Statements {
    pub contexts: BTreeMap<u32, Context>,
    pub docs: BTreeSet<u32>,
    pub ignored_fstrings: BTreeSet<u32>,
}

struct Collector<'a> {
    source: &'a str,
    index: LineIndex,
    data: Statements,
    parent: Parent,
    attribute_docs: bool,
}

impl Collector<'_> {
    fn line(&self, range: TextRange, end: bool) -> u32 {
        self.index
            .line_column(if end { range.end() } else { range.start() }, self.source)
            .line
            .get() as u32
    }
    fn with_parent(&mut self, body: &[Stmt], parent: Parent) {
        let previous = self.parent;
        self.parent = parent;
        self.visit_body(body);
        self.parent = previous;
    }
}

impl<'a> Visitor<'a> for Collector<'_> {
    fn visit_body(&mut self, body: &'a [Stmt]) {
        let skip = usize::from(
            matches!(
                self.parent.kind,
                Kind::Module | Kind::Class | Kind::Function
            ) && super::docstring(body),
        );
        if skip == 1 {
            self.data.docs.insert(body[0].start().to_u32());
        }
        let actual = &body[skip..];
        for (index, statement) in actual.iter().enumerate() {
            let previous = index.checked_sub(1).map(|index| &actual[index]);
            let attribute_doc = self.attribute_docs
                && previous.is_some_and(|stmt| {
                    matches!(
                        stmt,
                        Stmt::Assign(_) | Stmt::AnnAssign(_) | Stmt::TypeAlias(_)
                    )
                });
            let mut next = actual.get(index + 1);
            if matches!(statement, Stmt::Return(_)) && next.is_some_and(|stmt| matches!(stmt, Stmt::Expr(expr) if matches!(expr.value.as_ref(), Expr::Yield(_)))) { next = actual.get(index + 2); }
            self.data.contexts.insert(
                statement.start().to_u32(),
                Context {
                    parent: self.parent.kind,
                    misplaced_future: previous.is_some_and(|value| !matches!(value, Stmt::ImportFrom(value) if value.module.as_ref().is_some_and(|name|name.as_str()=="__future__"))),
                    previous_line: previous
                        .map_or(self.parent.line, |stmt| self.line(stmt.range(), false)),
                    siblings: actual.len(),
                    doc: self.parent.doc,
                    attribute_doc,
                    next: next.map(Ranged::range),
                },
            );
            self.visit_stmt(statement);
        }
    }
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        let previous = self.parent;
        let previous_attribute_docs = self.attribute_docs;
        let (kind, doc) = match statement {
            Stmt::FunctionDef(function) => {
                self.attribute_docs = function.name.as_str() == "__init__";
                (Kind::Function, super::docstring(&function.body))
            }
            Stmt::ClassDef(class) => {
                self.attribute_docs = true;
                (Kind::Class, super::docstring(&class.body))
            }
            _ => (Kind::Other, false),
        };
        self.parent = Parent {
            kind,
            line: self.line(statement.range(), false),
            doc,
        };
        if let Stmt::Try(attempt) = statement {
            self.with_parent(
                &attempt.body,
                Parent {
                    kind: Kind::Try,
                    ..self.parent
                },
            );
            for handler in &attempt.handlers {
                let ExceptHandler::ExceptHandler(handler) = handler;
                self.with_parent(
                    &handler.body,
                    Parent {
                        kind: Kind::Other,
                        line: self.line(handler.range(), false),
                        doc: false,
                    },
                );
            }
            let preceding = attempt
                .handlers
                .last()
                .map(|handler| {
                    let ExceptHandler::ExceptHandler(handler) = handler;
                    handler.body.as_slice()
                })
                .unwrap_or(&attempt.body);
            let line = preceding
                .last()
                .map_or(0, |stmt| self.line(stmt.range(), true) + 1);
            self.with_parent(
                &attempt.orelse,
                Parent {
                    kind: Kind::Other,
                    line,
                    doc: false,
                },
            );
            let preceding = if attempt.orelse.is_empty() {
                preceding
            } else {
                &attempt.orelse
            };
            let line = preceding
                .last()
                .map_or(0, |stmt| self.line(stmt.range(), true) + 1);
            self.with_parent(
                &attempt.finalbody,
                Parent {
                    kind: Kind::Other,
                    line,
                    doc: false,
                },
            );
        } else {
            walk_stmt(self, statement);
        }
        self.parent = previous;
        self.attribute_docs = previous_attribute_docs;
    }
    fn visit_expr(&mut self, expression: &'a Expr) {
        walk_expr(self, expression);
    }
    fn visit_interpolated_string_element(&mut self, element: &'a InterpolatedStringElement) {
        if let InterpolatedStringElement::Interpolation(interpolation) = element {
            if matches!(interpolation.expression.as_ref(), Expr::FString(_)) {
                self.data
                    .ignored_fstrings
                    .insert(interpolation.expression.start().to_u32());
            }
            self.visit_expr(&interpolation.expression);
            if let Some(spec) = &interpolation.format_spec {
                for element in &spec.elements {
                    self.visit_interpolated_string_element(element);
                }
            }
        }
    }
}

impl Statements {
    pub(super) fn new(body: &[Stmt], source: &str) -> Self {
        let mut collector = Collector {
            source,
            index: LineIndex::from_source_text(source),
            data: Self::default(),
            parent: Parent {
                kind: Kind::Module,
                line: 0,
                doc: super::docstring(body),
            },
            attribute_docs: true,
        };
        collector.visit_body(body);
        collector.data
    }
}
