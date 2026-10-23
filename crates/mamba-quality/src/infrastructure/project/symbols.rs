//! Syntax declarations are shared; each tool decides lookup and inference policy.
use super::{imports, Import};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    ExceptHandler, Expr, Parameters, Stmt,
};
use ruff_text_size::{Ranged, TextRange};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
pub(crate) enum DefinitionKind {
    Value,
    Function,
    Class,
    Import(Import),
    Parameter { variadic: bool },
}

#[derive(Clone)]
pub(crate) struct Definition {
    pub range: TextRange,
    pub kind: DefinitionKind,
}

#[derive(Default)]
pub(crate) struct Symbols {
    pub bindings: BTreeMap<String, Definition>,
    pub globals: BTreeSet<String>,
    pub nonlocals: BTreeSet<String>,
}

impl Symbols {
    pub(crate) fn collect(body: &[Stmt], current: &str, is_package: bool) -> Self {
        let mut collector = Collector {
            symbols: Self::default(),
            current,
            is_package,
        };
        for statement in body {
            collector.visit_stmt(statement);
        }
        collector.symbols
    }

    pub(crate) fn parameters(&mut self, parameters: &Parameters) {
        for parameter in parameters.iter() {
            self.bindings.insert(
                parameter.name().to_string(),
                Definition {
                    range: parameter.name().range(),
                    kind: DefinitionKind::Parameter {
                        variadic: parameter.is_variadic(),
                    },
                },
            );
        }
    }

    pub(crate) fn pattern(&mut self, target: &Expr) {
        match target {
            Expr::Name(name) => {
                self.bindings.insert(
                    name.id.to_string(),
                    Definition {
                        range: TextRange::default(),
                        kind: DefinitionKind::Value,
                    },
                );
            }
            Expr::Tuple(tuple) => {
                for item in &tuple.elts {
                    self.pattern(item);
                }
            }
            Expr::List(list) => {
                for item in &list.elts {
                    self.pattern(item);
                }
            }
            Expr::Starred(starred) => self.pattern(&starred.value),
            _ => {}
        }
    }
}

struct Collector<'a> {
    symbols: Symbols,
    current: &'a str,
    is_package: bool,
}

impl Collector<'_> {
    fn define(&mut self, name: &str, range: TextRange, kind: DefinitionKind) {
        self.symbols
            .bindings
            .entry(name.into())
            .or_insert(Definition { range, kind });
    }
}

impl<'a> Visitor<'a> for Collector<'_> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::FunctionDef(function) => self.define(
                function.name.as_str(),
                TextRange::new(function.start(), function.name.end()),
                DefinitionKind::Function,
            ),
            Stmt::ClassDef(class) => {
                self.define(
                    class.name.as_str(),
                    class.name.range(),
                    DefinitionKind::Class,
                );
            }
            Stmt::Import(_) | Stmt::ImportFrom(_) => {
                for import in imports(
                    std::slice::from_ref(statement),
                    self.current,
                    self.is_package,
                ) {
                    self.define(
                        &import.local,
                        import.range,
                        DefinitionKind::Import(import.clone()),
                    );
                }
            }
            Stmt::Global(statement) => self
                .symbols
                .globals
                .extend(statement.names.iter().map(ToString::to_string)),
            Stmt::Nonlocal(statement) => self
                .symbols
                .nonlocals
                .extend(statement.names.iter().map(ToString::to_string)),
            _ => walk_stmt(self, statement),
        }
    }

    fn visit_expr(&mut self, expression: &'a Expr) {
        match expression {
            Expr::Name(name) if name.ctx.is_store() => {
                self.define(name.id.as_str(), name.range(), DefinitionKind::Value);
            }
            Expr::Lambda(_)
            | Expr::ListComp(_)
            | Expr::SetComp(_)
            | Expr::DictComp(_)
            | Expr::Generator(_) => {}
            _ => walk_expr(self, expression),
        }
    }

    fn visit_except_handler(&mut self, handler: &'a ExceptHandler) {
        let ExceptHandler::ExceptHandler(handler) = handler;
        if let Some(name) = &handler.name {
            self.define(name.as_str(), handler.range(), DefinitionKind::Value);
        }
        if let Some(type_) = &handler.type_ {
            self.visit_expr(type_);
        }
        self.visit_body(&handler.body);
    }
}
