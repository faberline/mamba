//! Statement starts record physical indentation independently of continuation lines.
use mamba_language::application::quality_syntax::{
    self,
    python_ast::{
        visitor::{walk_match_case, walk_stmt, Visitor},
        MatchCase, Stmt,
    },
};
use ruff_text_size::Ranged;
use std::collections::BTreeMap;
struct Collector<'a, 'b> {
    document: &'a quality_syntax::SyntaxDocument<'b>,
    index: ruff_source_file::LineIndex,
    level: usize,
    lines: BTreeMap<u32, usize>,
}
impl<'a> Visitor<'a> for Collector<'_, '_> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        let start = statement.start().to_usize();
        let line_start = self.document.source[..start]
            .rfind(['\r', '\n'])
            .map_or(0, |index| index + 1);
        let prefix = self.document.source[line_start..start].trim_start();
        let clause = ["else", "elif", "except", "finally", "case"]
            .iter()
            .any(|keyword| {
                prefix.strip_prefix(keyword).is_some_and(|value| {
                    value.starts_with(':') || value.chars().next().is_some_and(char::is_whitespace)
                })
            });
        let level = self.level.saturating_sub(usize::from(clause));
        self.lines
            .entry(
                self.index
                    .line_column(statement.start(), self.document.source)
                    .line
                    .get() as u32,
            )
            .or_insert(level);
        let nested = matches!(
            statement,
            Stmt::ClassDef(_)
                | Stmt::FunctionDef(_)
                | Stmt::If(_)
                | Stmt::For(_)
                | Stmt::While(_)
                | Stmt::Try(_)
                | Stmt::With(_)
                | Stmt::Match(_)
        );
        self.level += usize::from(nested);
        walk_stmt(self, statement);
        self.level -= usize::from(nested);
    }
    fn visit_match_case(&mut self, case: &'a MatchCase) {
        self.lines
            .entry(
                self.index
                    .line_column(case.start(), self.document.source)
                    .line
                    .get() as u32,
            )
            .or_insert(self.level);
        self.level += 1;
        walk_match_case(self, case);
        self.level -= 1;
    }
}
pub(super) fn expected(document: &quality_syntax::SyntaxDocument<'_>) -> BTreeMap<u32, usize> {
    let mut value = Collector {
        document,
        index: ruff_source_file::LineIndex::from_source_text(document.source),
        level: 0,
        lines: BTreeMap::new(),
    };
    value.visit_body(&document.parsed.syntax().body);
    value.lines
}
