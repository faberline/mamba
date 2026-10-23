//! Module naming distinguishes reassignment from mutually exclusive definitions.
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    Expr, Stmt,
};
use ruff_text_size::Ranged;
use std::collections::{BTreeMap, BTreeSet};

struct Definition {
    line: u32,
    branches: Vec<(u32, usize)>,
}

struct Collector<'a> {
    source: &'a str,
    names: BTreeMap<String, Vec<Definition>>,
    branches: Vec<(u32, usize)>,
}

impl Collector<'_> {
    fn define(&mut self, name: &str, offset: u32) {
        let line = self.source[..offset as usize]
            .bytes()
            .filter(|&b| b == b'\n')
            .count() as u32;
        self.names.entry(name.into()).or_default().push(Definition {
            line,
            branches: self.branches.clone(),
        });
    }
    fn target(&mut self, expression: &Expr) {
        match expression {
            Expr::Name(name) => self.define(name.id.as_str(), name.start().to_u32()),
            Expr::Tuple(tuple) => {
                for item in &tuple.elts {
                    self.target(item);
                }
            }
            Expr::List(list) => {
                for item in &list.elts {
                    self.target(item);
                }
            }
            Expr::Starred(starred) => self.target(&starred.value),
            _ => {}
        }
    }
}

impl<'a> Visitor<'a> for Collector<'_> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::FunctionDef(function) => {
                self.define(function.name.as_str(), function.name.start().to_u32())
            }
            Stmt::ClassDef(class) => self.define(class.name.as_str(), class.name.start().to_u32()),
            Stmt::Assign(assignment) => {
                for target in &assignment.targets {
                    self.target(target);
                }
            }
            Stmt::AnnAssign(assignment) => self.target(&assignment.target),
            Stmt::AugAssign(assignment) => self.target(&assignment.target),
            Stmt::For(loop_statement) => {
                self.target(&loop_statement.target);
                walk_stmt(self, statement);
            }
            Stmt::If(conditional) => {
                let id = conditional.start().to_u32();
                self.branches.push((id, 0));
                self.visit_body(&conditional.body);
                for (index, clause) in conditional.elif_else_clauses.iter().enumerate() {
                    *self.branches.last_mut().unwrap() = (id, index + 1);
                    self.visit_body(&clause.body);
                }
                self.branches.pop();
            }
            _ => walk_stmt(self, statement),
        }
    }
    fn visit_expr(&mut self, _expression: &'a Expr) {}
}

pub(super) fn reassigned_names(body: &[Stmt], source: &str) -> BTreeSet<String> {
    let mut collector = Collector {
        source,
        names: BTreeMap::new(),
        branches: Vec::new(),
    };
    collector.visit_body(body);
    collector
        .names
        .into_iter()
        .filter_map(|(name, definitions)| {
            let mut mutable = false;
            for (index, definition) in definitions.iter().enumerate() {
                for other in &definitions[index + 1..] {
                    let exclusive = definition.branches.iter().any(|(id, arm)| {
                        other
                            .branches
                            .iter()
                            .any(|(other_id, other_arm)| id == other_id && arm != other_arm)
                    });
                    mutable |= definition.line != other.line && !exclusive;
                }
            }
            mutable.then_some(name)
        })
        .collect()
}

pub(super) fn import_fallback(body: &[Stmt], name: &str, offset: u32) -> bool {
    #[derive(Default)]
    struct Imported(BTreeSet<String>);
    impl<'ast> Visitor<'ast> for Imported {
        fn visit_stmt(&mut self, statement: &'ast Stmt) {
            let aliases = match statement {
                Stmt::Import(value) => Some(&value.names),
                Stmt::ImportFrom(value) => Some(&value.names),
                _ => None,
            };
            if let Some(aliases) = aliases {
                for alias in aliases {
                    self.0
                        .insert(alias.asname.as_ref().unwrap_or(&alias.name).to_string());
                }
            }
            walk_stmt(self, statement);
        }
    }
    struct Fallback<'a> {
        name: &'a str,
        offset: u32,
        found: bool,
    }
    impl<'ast> Visitor<'ast> for Fallback<'_> {
        fn visit_stmt(&mut self, statement: &'ast Stmt) {
            if let Stmt::Try(attempt) = statement {
                let mut imports = Imported::default();
                imports.visit_stmt(statement);
                if imports.0.contains(self.name) {
                    for handler in &attempt.handlers {
                        let mamba_language::application::quality_syntax::python_ast::ExceptHandler::ExceptHandler(handler) = handler;
                        let catches_import = handler.type_.as_deref().is_some_and(|kind| {
                            matches!(kind, Expr::Name(name) if name.id.as_str() == "ImportError")
                                || matches!(kind, Expr::Tuple(values) if values.elts.iter().any(|value| matches!(value, Expr::Name(name) if name.id.as_str() == "ImportError")))
                        });
                        self.found |=
                            catches_import && handler.range().contains(self.offset.into());
                    }
                }
            }
            walk_stmt(self, statement);
        }
    }
    let mut finder = Fallback {
        name,
        offset,
        found: false,
    };
    finder.visit_body(body);
    finder.found
}
