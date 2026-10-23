//! Imports retain source order and static target-version branch reachability.
use mamba_language::application::quality_syntax::{
    python_ast::{
        visitor::{walk_stmt, Visitor},
        Expr, Stmt,
    },
    PythonTarget,
};
use std::collections::BTreeSet;

pub(super) struct Imports<'a> {
    pub module: &'a str,
    pub package: bool,
    pub target: PythonTarget,
    pub names: Vec<String>,
    seen: BTreeSet<String>,
    pub entries: Option<Vec<super::output::Import>>,
    in_function: usize,
    mypy_only: Option<bool>,
}
impl<'a> Imports<'a> {
    pub(super) fn new(module: &'a str, package: bool, target: PythonTarget) -> Self {
        Self {
            module,
            package,
            target,
            names: Vec::new(),
            seen: BTreeSet::new(),
            entries: None,
            in_function: 0,
            mypy_only: Some(false),
        }
    }

    pub(super) fn for_output(module: &'a str, package: bool, target: PythonTarget) -> Self {
        Self {
            entries: Some(Vec::new()),
            ..Self::new(module, package, target)
        }
    }

    fn branch_mypy_only(&self, test: &Expr) -> Option<bool> {
        if self.entries.is_some() {
            super::output::mypy_only(test, self.target)
        } else {
            Some(false)
        }
    }

    fn conditional_body(&mut self, body: &[Stmt], mypy_only: Option<bool>) {
        let previous = self.mypy_only;
        self.mypy_only = match (previous, mypy_only) {
            (Some(true), _) | (_, Some(true)) => Some(true),
            (Some(false), Some(false)) => Some(false),
            _ => None,
        };
        self.visit_body(body);
        self.mypy_only = previous;
    }

    fn add(&mut self, name: String) {
        if self.seen.insert(name.clone()) {
            self.names.push(name);
        }
    }
}
impl<'ast> Visitor<'ast> for Imports<'_> {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        if let Stmt::If(conditional) = statement {
            let truth = super::super::classes::statements::truth(&conditional.test, self.target);
            if truth != Some(false) {
                self.conditional_body(&conditional.body, self.branch_mypy_only(&conditional.test));
            }
            if truth == Some(true) {
                return;
            }
            for clause in &conditional.elif_else_clauses {
                let truth = clause
                    .test
                    .as_ref()
                    .and_then(|test| super::super::classes::statements::truth(test, self.target));
                if truth != Some(false) {
                    let mypy_only = clause
                        .test
                        .as_ref()
                        .map_or(Some(false), |test| self.branch_mypy_only(test));
                    self.conditional_body(&clause.body, mypy_only);
                }
                if truth == Some(true) || clause.test.is_none() {
                    break;
                }
            }
            return;
        }
        let imports = crate::infrastructure::project::imports(
            std::slice::from_ref(statement),
            self.module,
            self.package,
        );
        if let (Some(entries), Some(first)) = (&mut self.entries, imports.first()) {
            if matches!(statement, Stmt::Import(_)) {
                entries.extend(imports.iter().map(|import| super::output::Import {
                    module: import.module.clone(),
                    members: None,
                    in_function: self.in_function > 0,
                    mypy_only: self.mypy_only,
                }));
            } else {
                entries.push(super::output::Import {
                    module: first.module.clone(),
                    members: Some(
                        imports
                            .iter()
                            .filter_map(|import| import.member.clone())
                            .collect(),
                    ),
                    in_function: self.in_function > 0,
                    mypy_only: self.mypy_only,
                });
            }
        }
        for import in imports {
            self.add(import.module.clone());
            if let Some(member) = import.member.filter(|member| member != "*") {
                self.add(format!("{}.{member}", import.module));
            }
        }
        let function = usize::from(matches!(statement, Stmt::FunctionDef(_)));
        self.in_function += function;
        walk_stmt(self, statement);
        self.in_function -= function;
    }
}
