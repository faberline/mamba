//! Redefinition checks retain first bindings, decorators, and exclusive branches.
use super::Checker;
use mamba_language::application::quality_syntax::python_ast::{Expr, Stmt, StmtFunctionDef};
use ruff_text_size::{TextRange, TextSize};
use std::collections::BTreeMap;
mod branches;
mod collector;
mod flow;

#[derive(Default)]
pub(super) struct Definitions(
    BTreeMap<u32, TextSize>,
    BTreeMap<u32, bool>,
    std::collections::BTreeSet<u32>,
);
impl Definitions {
    pub(super) fn new(
        body: &[Stmt],
        target: mamba_language::application::quality_syntax::PythonTarget,
    ) -> Self {
        let mut result = Self::default();
        collector::scope(
            body,
            None,
            false,
            &BTreeMap::new(),
            target,
            &mut result.0,
            &mut result.1,
            &mut result.2,
        );
        result
    }
    pub(super) fn conditional_use(&self, range: TextRange) -> Option<bool> {
        self.1.get(&range.start().to_u32()).copied()
    }
    pub(super) fn expired_handler_use(&self, range: TextRange) -> bool {
        self.2.contains(&range.start().to_u32())
    }
}

pub(super) fn decorator_redefines(function: &StmtFunctionDef) -> bool {
    function.decorator_list.iter().any(|value| {
        matches!(&value.expression, Expr::Attribute(value) if matches!(value.value.as_ref(), Expr::Name(name) if name.id.as_str() == function.name.as_str()))
    })
}

impl Checker<'_> {
    pub(super) fn check_redefinition(&mut self, name: TextRange, kind: &str) {
        if self.module_name == "threading"
            && self.scopes.current_index() == 0
            && &self.source.text[name] == "Lock"
        {
            // Astroid's pinned module extender adds its Lock declaration on line 14.
            if kind != "class" {
                return;
            }
            self.emit(
                "E0102",
                "function-redefined",
                &format!("{kind} already defined line 14"),
                self.declaration_range(name),
            );
            return;
        }
        if let Some(first) = self.definitions.0.get(&name.start().to_u32()) {
            let line = ruff_source_file::LineIndex::from_source_text(&self.source.text)
                .line_column(*first, &self.source.text)
                .line
                .get();
            self.emit(
                "E0102",
                "function-redefined",
                &format!("{kind} already defined line {line}"),
                self.declaration_range(name),
            );
        }
    }
}
