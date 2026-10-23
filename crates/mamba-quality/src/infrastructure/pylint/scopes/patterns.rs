//! Match captures enter the Pylint lexical frame before case bodies are walked.
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_pattern, walk_stmt, Visitor},
    Pattern, Stmt,
};
use ruff_text_size::{Ranged, TextRange};
use std::collections::BTreeMap;
#[derive(Default)]
struct Captures(BTreeMap<String, TextRange>);
impl<'a> Visitor<'a> for Captures {
    fn visit_stmt(&mut self, value: &'a Stmt) {
        if !matches!(value, Stmt::FunctionDef(_) | Stmt::ClassDef(_)) {
            walk_stmt(self, value);
        }
    }
    fn visit_pattern(&mut self, value: &'a Pattern) {
        let name = match value {
            Pattern::MatchAs(value) => value.name.as_ref(),
            Pattern::MatchStar(value) => value.name.as_ref(),
            Pattern::MatchMapping(value) => value.rest.as_ref(),
            _ => None,
        };
        if let Some(name) = name {
            let range = if matches!(value, Pattern::MatchAs(_)) {
                value.range()
            } else {
                name.range
            };
            self.0.entry(name.to_string()).or_insert(range);
        }
        walk_pattern(self, value);
    }
}
pub(in crate::infrastructure::pylint) fn collect(body: &[Stmt]) -> BTreeMap<String, TextRange> {
    let mut values = Captures::default();
    values.visit_body(body);
    values.0
}
