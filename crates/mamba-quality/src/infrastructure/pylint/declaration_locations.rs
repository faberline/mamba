//! Astroid declaration headers retain character columns and normalized-name fallback.
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    Stmt,
};
use ruff_text_size::{Ranged, TextRange, TextSize};
use std::collections::BTreeMap;
#[derive(Default)]
pub(super) struct Locations {
    ranges: BTreeMap<(u32, u32), TextRange>,
    columns: BTreeMap<(u32, u32), u32>,
}
struct Collector<'a> {
    text: &'a str,
    locations: Locations,
}
impl Collector<'_> {
    fn add(&mut self, name: &str, name_range: TextRange, statement: TextRange) {
        let before = &self.text[..name_range.start().to_usize()];
        let line = before.rfind('\n').map_or(0, |offset| offset + 1);
        let indent = before[line..].len() - before[line..].trim_start().len();
        let range = if &self.text[name_range] == name {
            TextRange::new(TextSize::try_from(line + indent).unwrap(), name_range.end())
        } else {
            statement
        };
        self.locations.ranges.insert(key(name_range), range);
        if &self.text[name_range] == name {
            self.locations.columns.insert(
                key(range),
                (before[line..].chars().count() + name.chars().count()) as u32,
            );
        }
    }
}
impl<'a> Visitor<'a> for Collector<'_> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::FunctionDef(value) => {
                self.add(value.name.as_str(), value.name.range(), value.range())
            }
            Stmt::ClassDef(value) => {
                self.add(value.name.as_str(), value.name.range(), value.range())
            }
            _ => {}
        }
        walk_stmt(self, statement);
    }
}
impl Locations {
    pub(super) fn new(body: &[Stmt], text: &str) -> Self {
        let mut collector = Collector {
            text,
            locations: Self::default(),
        };
        collector.visit_body(body);
        collector.locations
    }
    pub(super) fn range(&self, name: TextRange) -> Option<TextRange> {
        self.ranges.get(&key(name)).copied()
    }
    pub(super) fn column(&self, range: TextRange) -> Option<u32> {
        self.columns.get(&key(range)).copied()
    }
}

fn key(range: TextRange) -> (u32, u32) {
    (range.start().to_u32(), range.end().to_u32())
}
