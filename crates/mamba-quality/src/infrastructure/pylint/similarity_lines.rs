//! Similarity input retains physical line numbers and the fixed exclusions.
use super::{directives, source_layout};
use mamba_language::application::quality_syntax::{
    python_ast::{
        visitor::{walk_stmt, Visitor},
        Stmt,
    },
    SyntaxDocument,
};
use ruff_text_size::Ranged;
use std::collections::BTreeSet;

pub(super) struct Lines<'a> {
    pub real: Vec<&'a str>,
    pub stripped: Vec<(usize, String)>,
}
struct Imports<'a, 'b> {
    document: &'a SyntaxDocument<'b>,
    ignored: BTreeSet<u32>,
}
impl<'a> Visitor<'a> for Imports<'_, '_> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if matches!(statement, Stmt::Import(_) | Stmt::ImportFrom(_)) {
            let start = self.document.line_column(statement.start().to_u32()).0;
            let end = self.document.line_column(statement.end().to_u32()).0;
            self.ignored.extend(start..=end);
        }
        walk_stmt(self, statement);
    }
}
fn signatures(body: &[Stmt], document: &SyntaxDocument<'_>, ignored: &mut BTreeSet<u32>) {
    for statement in body {
        match statement {
            Stmt::FunctionDef(function) => {
                let start = document.line_column(function.start().to_u32()).0;
                let end = function.body.first().map_or_else(
                    || document.line_column(function.end().to_u32()).0 + 1,
                    |first| document.line_column(first.start().to_u32()).0,
                );
                ignored.extend(start..end);
                signatures(&function.body, document, ignored);
            }
            Stmt::ClassDef(class) => signatures(&class.body, document, ignored),
            _ => {}
        }
    }
}
impl<'a> Lines<'a> {
    pub(super) fn new(source: &'a str, document: &SyntaxDocument<'_>) -> Self {
        let body = &document.parsed.syntax().body;
        let mut imports = Imports {
            document,
            ignored: BTreeSet::new(),
        };
        imports.visit_body(body);
        signatures(body, document, &mut imports.ignored);
        let instructions = directives::instructions(document);
        let real: Vec<_> = source_layout::lines(source)
            .into_iter()
            .map(|(_, line)| line)
            .collect();
        let mut stripped = Vec::new();
        let mut docstring: Option<&str> = None;
        for (index, line) in real.iter().enumerate() {
            let number = (index + 1) as u32;
            if directives::suppressed(&instructions, "R0801", "duplicate-code", number) {
                continue;
            }
            let mut text = line.trim_matches(source_layout::whitespace);
            if docstring.is_none() {
                if text.starts_with("\"\"\"") || text.starts_with("'''") {
                    docstring = Some(&text[..3]);
                    text = &text[3..];
                } else if text.starts_with("r\"\"\"") || text.starts_with("r'''") {
                    docstring = Some(&text[1..4]);
                    text = &text[4..];
                }
            }
            if let Some(delimiter) = docstring {
                if text.ends_with(delimiter) {
                    docstring = None;
                }
                continue;
            }
            text = text.split('#').next().unwrap_or_default().trim();
            if !text.is_empty() && !imports.ignored.contains(&number) {
                stripped.push((index, text.into()));
            }
        }
        Self { real, stripped }
    }
    pub(super) fn end(&self, start: usize, count: usize) -> usize {
        self.stripped
            .get(start + count)
            .map_or_else(|| self.stripped.last().unwrap().0 + 1, |line| line.0)
    }
}
