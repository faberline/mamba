//! Attribute writes retain lexical function identity without executing a body.
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    Expr, Stmt,
};
use ruff_text_size::Ranged;
use std::collections::BTreeMap;

#[derive(Clone)]
pub(super) enum Owner {
    Function(u32, String),
    Path(Vec<String>),
    Unknown,
}
pub(super) struct Write {
    pub owner: Owner,
    pub name: String,
}
struct Scan {
    bindings: BTreeMap<String, Owner>,
    writes: Vec<Write>,
    module: String,
    package: bool,
}
impl Scan {
    fn expression(&self, value: &Expr) -> Owner {
        if let Expr::Lambda(value) = value {
            return Owner::Function(value.start().to_u32(), "<lambda>".into());
        }
        let Some(path) = super::super::values::path(value) else {
            return Owner::Unknown;
        };
        match self.bindings.get(&path[0]) {
            Some(Owner::Function(offset, name)) if path.len() == 1 => {
                Owner::Function(*offset, name.clone())
            }
            Some(Owner::Path(prefix)) => Owner::Path(
                prefix
                    .iter()
                    .cloned()
                    .chain(path.into_iter().skip(1))
                    .collect(),
            ),
            Some(_) => Owner::Unknown,
            None => Owner::Path(path),
        }
    }
    fn define(&mut self, target: &Expr, owner: Owner) {
        if let Expr::Name(value) = target {
            self.bindings.insert(value.id.to_string(), owner);
        }
    }
    fn target(&mut self, target: &Expr) {
        match target {
            Expr::Attribute(value) => self.writes.push(Write {
                owner: self.expression(&value.value),
                name: value.attr.to_string(),
            }),
            Expr::Tuple(value) => value.elts.iter().for_each(|value| self.target(value)),
            Expr::List(value) => value.elts.iter().for_each(|value| self.target(value)),
            Expr::Starred(value) => self.target(&value.value),
            _ => {}
        }
    }
    fn scope(&mut self, body: &[Stmt], parameters: &[String]) {
        let previous = self.bindings.clone();
        for name in parameters {
            self.bindings.insert(name.clone(), Owner::Unknown);
        }
        struct Declarations(Vec<(String, Owner)>);
        impl<'a> Visitor<'a> for Declarations {
            fn visit_stmt(&mut self, statement: &'a Stmt) {
                match statement {
                    Stmt::FunctionDef(value) => self.0.push((
                        value.name.to_string(),
                        Owner::Function(value.name.start().to_u32(), value.name.to_string()),
                    )),
                    Stmt::ClassDef(value) => self.0.push((value.name.to_string(), Owner::Unknown)),
                    _ => walk_stmt(self, statement),
                }
            }
        }
        let mut declarations = Declarations(Vec::new());
        declarations.visit_body(body);
        self.bindings.extend(declarations.0);
        self.visit_body(body);
        self.bindings = previous;
    }
}
impl<'a> Visitor<'a> for Scan {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::FunctionDef(value) => {
                let parameters = value
                    .parameters
                    .iter()
                    .map(|value| value.name().to_string())
                    .collect::<Vec<_>>();
                self.scope(&value.body, &parameters);
                return;
            }
            Stmt::ClassDef(value) => {
                self.scope(&value.body, &[]);
                return;
            }
            Stmt::Assign(value) => {
                let owner = self.expression(&value.value);
                for target in &value.targets {
                    self.target(target);
                    self.define(target, owner.clone());
                }
            }
            Stmt::AnnAssign(value) => {
                self.target(&value.target);
                if let Some(value_) = &value.value {
                    self.define(&value.target, self.expression(value_));
                }
            }
            Stmt::Import(_) | Stmt::ImportFrom(_) => {
                for import in crate::infrastructure::project::imports(
                    std::slice::from_ref(statement),
                    &self.module,
                    self.package,
                ) {
                    let mut path = import
                        .module
                        .split('.')
                        .map(ToString::to_string)
                        .collect::<Vec<_>>();
                    path.extend(import.member);
                    self.bindings.insert(import.local, Owner::Path(path));
                }
            }
            _ => {}
        }
        walk_stmt(self, statement);
    }
}
pub(super) fn collect(body: &[Stmt], module: &str, package: bool) -> Vec<Write> {
    let mut scan = Scan {
        bindings: BTreeMap::new(),
        writes: Vec::new(),
        module: module.into(),
        package,
    };
    scan.scope(body, &[]);
    scan.writes
}
